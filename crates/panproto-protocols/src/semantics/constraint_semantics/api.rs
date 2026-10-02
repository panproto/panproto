//! Constraint-semantics rows for the API description protocols (`openapi`,
//! `asyncapi`, `jsonapi`, `raml`, `graphql`).
//!
//! The `openapi`, `asyncapi` and `raml` schema keywords are JSON Schema's: string
//! lengths count Unicode code points, `minItems` and `maxItems` bound the
//! items of a list (the out-edges of its node), and `format` names a value
//! format of the protocol's family. `default` and `deprecated` describe and
//! do not constrain. The JSON:API `required` sort is written both as a
//! presence flag of an attribute and as the target type of a relationship,
//! and GraphQL's `non_null` and `list` record its type wrappers; each is
//! descriptive here. A presence flag is about the element that holds the
//! property, which must have it (the schema's required edge of that
//! element), not about the property's own value: a present property of
//! record or list type has no value of its own, so reading the flag as a
//! required value would refute every valid instance that holds one.

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
    row("openapi", "required", || ConstraintSemantics::Annotation),
    row("openapi", "format", || {
        ConstraintSemantics::Format(Name::from("openapi"))
    }),
    row("openapi", "enum", || ConstraintSemantics::Enum),
    row("openapi", "default", || ConstraintSemantics::Annotation),
    row("openapi", "minimum", || ConstraintSemantics::Minimum),
    row("openapi", "maximum", || ConstraintSemantics::Maximum),
    row("openapi", "pattern", || ConstraintSemantics::Pattern),
    row("openapi", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("openapi", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("openapi", "minItems", || ConstraintSemantics::MinOutDegree),
    row("openapi", "maxItems", || ConstraintSemantics::MaxOutDegree),
    row("openapi", "deprecated", || ConstraintSemantics::Annotation),
    row("asyncapi", "required", || ConstraintSemantics::Annotation),
    row("asyncapi", "format", || {
        ConstraintSemantics::Format(Name::from("asyncapi"))
    }),
    row("asyncapi", "enum", || ConstraintSemantics::Enum),
    row("asyncapi", "default", || ConstraintSemantics::Annotation),
    row("jsonapi", "required", || ConstraintSemantics::Annotation),
    row("raml", "required", || ConstraintSemantics::Annotation),
    row("raml", "pattern", || ConstraintSemantics::Pattern),
    row("raml", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("raml", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("raml", "minimum", || ConstraintSemantics::Minimum),
    row("raml", "maximum", || ConstraintSemantics::Maximum),
    row("raml", "enum", || ConstraintSemantics::Enum),
    row("raml", "default", || ConstraintSemantics::Annotation),
    row("graphql", "non_null", || ConstraintSemantics::Annotation),
    row("graphql", "list", || ConstraintSemantics::Annotation),
    row("graphql", "deprecated", || ConstraintSemantics::Annotation),
];
