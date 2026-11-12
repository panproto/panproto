//! Constraint-semantics rows for the API description protocols (`openapi`,
//! `asyncapi`, `jsonapi`, `raml`, `graphql`).
//!
//! An `openapi` constraint holds a field's value, or a Schema Object
//! keyword's value, as canonical JSON text. The Schema Object keywords read
//! as they do under `json-schema`: string lengths count code points and
//! `minimum` and `maximum` bound inclusively, and every other keyword
//! describes. Every field of the description's other objects (titles,
//! versions, parameter locations and styles, examples, scopes, the
//! specification extensions) describes the API and constrains no instance.
//!
//! The `asyncapi` and `raml` schema keywords are JSON Schema's: string
//! lengths count Unicode code points and `format` names a value format of
//! the protocol's family. `default` and `deprecated` describe and do not
//! constrain. The JSON:API `required` sort is written both as a
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
    row("openapi", "openapi", || ConstraintSemantics::Annotation),
    row("openapi", "jsonSchemaDialect", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "servers", || ConstraintSemantics::Annotation),
    row("openapi", "webhooks", || ConstraintSemantics::Annotation),
    row("openapi", "security", || ConstraintSemantics::Annotation),
    row("openapi", "tags", || ConstraintSemantics::Annotation),
    row("openapi", "title", || ConstraintSemantics::Annotation),
    row("openapi", "summary", || ConstraintSemantics::Annotation),
    row("openapi", "description", || ConstraintSemantics::Annotation),
    row("openapi", "termsOfService", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "version", || ConstraintSemantics::Annotation),
    row("openapi", "name", || ConstraintSemantics::Annotation),
    row("openapi", "url", || ConstraintSemantics::Annotation),
    row("openapi", "email", || ConstraintSemantics::Annotation),
    row("openapi", "identifier", || ConstraintSemantics::Annotation),
    row("openapi", "variables", || ConstraintSemantics::Annotation),
    row("openapi", "enum", || ConstraintSemantics::Annotation),
    row("openapi", "default", || ConstraintSemantics::Annotation),
    row("openapi", "schemas", || ConstraintSemantics::Annotation),
    row("openapi", "responses", || ConstraintSemantics::Annotation),
    row("openapi", "parameters", || ConstraintSemantics::Annotation),
    row("openapi", "examples", || ConstraintSemantics::Annotation),
    row("openapi", "requestBodies", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "headers", || ConstraintSemantics::Annotation),
    row("openapi", "securitySchemes", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "links", || ConstraintSemantics::Annotation),
    row("openapi", "callbacks", || ConstraintSemantics::Annotation),
    row("openapi", "pathItems", || ConstraintSemantics::Annotation),
    row("openapi", "operationId", || ConstraintSemantics::Annotation),
    row("openapi", "deprecated", || ConstraintSemantics::Annotation),
    row("openapi", "in", || ConstraintSemantics::Annotation),
    row("openapi", "required", || ConstraintSemantics::Annotation),
    row("openapi", "allowEmptyValue", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "style", || ConstraintSemantics::Annotation),
    row("openapi", "explode", || ConstraintSemantics::Annotation),
    row("openapi", "allowReserved", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "example", || ConstraintSemantics::Annotation),
    row("openapi", "content", || ConstraintSemantics::Annotation),
    row("openapi", "encoding", || ConstraintSemantics::Annotation),
    row("openapi", "contentType", || ConstraintSemantics::Annotation),
    row("openapi", "value", || ConstraintSemantics::Annotation),
    row("openapi", "externalValue", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "operationRef", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "requestBody", || ConstraintSemantics::Annotation),
    row("openapi", "type", || ConstraintSemantics::Annotation),
    row("openapi", "scheme", || ConstraintSemantics::Annotation),
    row("openapi", "bearerFormat", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "openIdConnectUrl", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "authorizationUrl", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "tokenUrl", || ConstraintSemantics::Annotation),
    row("openapi", "refreshUrl", || ConstraintSemantics::Annotation),
    row("openapi", "scopes", || ConstraintSemantics::Annotation),
    row("openapi", "extension", || ConstraintSemantics::Annotation),
    row("openapi", "ignored", || ConstraintSemantics::Annotation),
    row("openapi", "$schema", || ConstraintSemantics::Annotation),
    row("openapi", "$id", || ConstraintSemantics::Annotation),
    row("openapi", "$anchor", || ConstraintSemantics::Annotation),
    row("openapi", "$dynamicAnchor", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "$vocabulary", || ConstraintSemantics::Annotation),
    row("openapi", "$comment", || ConstraintSemantics::Annotation),
    row("openapi", "const", || ConstraintSemantics::Annotation),
    row("openapi", "multipleOf", || ConstraintSemantics::Annotation),
    row("openapi", "maximum", || ConstraintSemantics::Maximum),
    row("openapi", "exclusiveMaximum", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "minimum", || ConstraintSemantics::Minimum),
    row("openapi", "exclusiveMinimum", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("openapi", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("openapi", "pattern", || ConstraintSemantics::Annotation),
    row("openapi", "maxItems", || ConstraintSemantics::Annotation),
    row("openapi", "minItems", || ConstraintSemantics::Annotation),
    row("openapi", "uniqueItems", || ConstraintSemantics::Annotation),
    row("openapi", "maxContains", || ConstraintSemantics::Annotation),
    row("openapi", "minContains", || ConstraintSemantics::Annotation),
    row("openapi", "maxProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "minProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "dependentRequired", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "format", || ConstraintSemantics::Annotation),
    row("openapi", "contentEncoding", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "contentMediaType", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "readOnly", || ConstraintSemantics::Annotation),
    row("openapi", "writeOnly", || ConstraintSemantics::Annotation),
    row("openapi", "$defs", || ConstraintSemantics::Annotation),
    row("openapi", "properties", || ConstraintSemantics::Annotation),
    row("openapi", "patternProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "dependentSchemas", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "annotation", || ConstraintSemantics::Annotation),
    row("openapi", "discriminator", || {
        ConstraintSemantics::Annotation
    }),
    row("openapi", "xml", || ConstraintSemantics::Annotation),
    row("openapi", "externalDocs", || {
        ConstraintSemantics::Annotation
    }),
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
