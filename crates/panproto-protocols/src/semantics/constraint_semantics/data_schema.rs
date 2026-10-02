//! Constraint-semantics rows for the data schema protocols (`json-schema`,
//! `bson`, `cddl`).
//!
//! JSON Schema counts a string's length in code points, compares `minimum`
//! and `maximum` inclusively, and names value formats with `format`. `type`
//! is the vertex kind itself. `additionalProperties`, `items` and
//! `additionalItems` set to `false` close a record or a tuple to the members
//! it declares, which validation already requires of every element, since
//! an edge the schema does not declare is a failure; they describe here.
//! `required` is a presence flag of a property, about the object that holds
//! it (the schema's required edge of the object), not about the property's
//! own value, which a record or list property does not have; it describes.
//! MongoDB's `$jsonSchema` keywords, which the `bson` protocol reads, are
//! JSON Schema's.
//!
//! CDDL's `.size` control bounds the byte length of a text or byte string,
//! or the byte width of an unsigned integer, and is written as a number or a
//! range; a CDDL range is a type, not a pair of bounds; and `.default` gives
//! a default value. No single meaning reads them, and the `cddl` parser
//! emits none of them, so they describe.

use panproto_gat::Name;
use panproto_schema::{ConstraintSemantics, LengthUnit};

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
    row("json-schema", "type", || ConstraintSemantics::Annotation),
    row("json-schema", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("json-schema", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("json-schema", "pattern", || ConstraintSemantics::Pattern),
    row("json-schema", "minimum", || ConstraintSemantics::Minimum),
    row("json-schema", "maximum", || ConstraintSemantics::Maximum),
    row("json-schema", "required", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "format", || {
        ConstraintSemantics::Format(Name::from("json-schema"))
    }),
    row("json-schema", "enum", || ConstraintSemantics::Enum),
    row("json-schema", "const", || ConstraintSemantics::Const),
    row("json-schema", "additionalProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "items", || ConstraintSemantics::Annotation),
    row("json-schema", "additionalItems", || {
        ConstraintSemantics::Annotation
    }),
    row("bson", "required", || ConstraintSemantics::Annotation),
    row("bson", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("bson", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("bson", "minimum", || ConstraintSemantics::Minimum),
    row("bson", "maximum", || ConstraintSemantics::Maximum),
    row("cddl", "size", || ConstraintSemantics::Annotation),
    row("cddl", "range", || ConstraintSemantics::Annotation),
    row("cddl", "default", || ConstraintSemantics::Annotation),
];
