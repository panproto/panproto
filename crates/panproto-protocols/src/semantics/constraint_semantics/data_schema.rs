//! Constraint-semantics rows for the data schema protocols (`json-schema`,
//! `bson`, `cddl`).
//!
//! A `json-schema` constraint holds its keyword's value as canonical JSON
//! text. JSON Schema 2020-12 counts a string's length in code points and
//! compares `minimum` and `maximum` inclusively, and a count written as an
//! integer and a bound written as a number read as those meanings. Every
//! other keyword describes, as does an empty object of subschemas kept
//! under its applicator's name: `format` is an annotation in the 2020-12
//! dialect unless a format-assertion vocabulary is declared, and the
//! applicators the other validation keywords combine with are edges of the
//! schema, which validation does not evaluate as JSON Schema does.
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
    row("json-schema", "$schema", || ConstraintSemantics::Annotation),
    row("json-schema", "$id", || ConstraintSemantics::Annotation),
    row("json-schema", "$anchor", || ConstraintSemantics::Annotation),
    row("json-schema", "$dynamicAnchor", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "$vocabulary", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "$comment", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "type", || ConstraintSemantics::Annotation),
    row("json-schema", "enum", || ConstraintSemantics::Annotation),
    row("json-schema", "const", || ConstraintSemantics::Annotation),
    row("json-schema", "multipleOf", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "maximum", || ConstraintSemantics::Maximum),
    row("json-schema", "exclusiveMaximum", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "minimum", || ConstraintSemantics::Minimum),
    row("json-schema", "exclusiveMinimum", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("json-schema", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("json-schema", "pattern", || ConstraintSemantics::Annotation),
    row("json-schema", "maxItems", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "minItems", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "uniqueItems", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "maxContains", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "minContains", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "maxProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "minProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "required", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "dependentRequired", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "format", || ConstraintSemantics::Annotation),
    row("json-schema", "contentEncoding", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "contentMediaType", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "title", || ConstraintSemantics::Annotation),
    row("json-schema", "description", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "default", || ConstraintSemantics::Annotation),
    row("json-schema", "deprecated", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "readOnly", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "writeOnly", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "examples", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "annotation", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "$defs", || ConstraintSemantics::Annotation),
    row("json-schema", "properties", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "patternProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("json-schema", "dependentSchemas", || {
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
