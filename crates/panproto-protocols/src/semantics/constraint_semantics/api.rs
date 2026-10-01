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
//! descriptive here (see the presence reading of the
//! [`common`](super::common) module).

use panproto_gat::Name;
use panproto_schema::ConstraintSemantics;

use super::ConstraintRow;
use super::common::{
    annotation, enumeration, max_chars, max_out_degree, maximum, min_chars, min_out_degree,
    minimum, pattern, row,
};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("openapi", "required", annotation),
    row("openapi", "format", || {
        ConstraintSemantics::Format(Name::from("openapi"))
    }),
    row("openapi", "enum", enumeration),
    row("openapi", "default", annotation),
    row("openapi", "minimum", minimum),
    row("openapi", "maximum", maximum),
    row("openapi", "pattern", pattern),
    row("openapi", "minLength", min_chars),
    row("openapi", "maxLength", max_chars),
    row("openapi", "minItems", min_out_degree),
    row("openapi", "maxItems", max_out_degree),
    row("openapi", "deprecated", annotation),
    row("asyncapi", "required", annotation),
    row("asyncapi", "format", || {
        ConstraintSemantics::Format(Name::from("asyncapi"))
    }),
    row("asyncapi", "enum", enumeration),
    row("asyncapi", "default", annotation),
    row("jsonapi", "required", annotation),
    row("raml", "required", annotation),
    row("raml", "pattern", pattern),
    row("raml", "minLength", min_chars),
    row("raml", "maxLength", max_chars),
    row("raml", "minimum", minimum),
    row("raml", "maximum", maximum),
    row("raml", "enum", enumeration),
    row("raml", "default", annotation),
    row("graphql", "non_null", annotation),
    row("graphql", "list", annotation),
    row("graphql", "deprecated", annotation),
];
