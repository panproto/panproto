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
//! An `asyncapi` constraint holds a field's value, a binding, or a Schema
//! Object keyword's value, as canonical JSON text, and reads as an
//! `openapi` one does: the Schema Object keywords bound string lengths in
//! code points and numbers inclusively, and every other sort describes the
//! API and constrains no instance.
//!
//! The `raml` schema keywords are JSON Schema's: string
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
    row("asyncapi", "title", || ConstraintSemantics::Annotation),
    row("asyncapi", "summary", || ConstraintSemantics::Annotation),
    row("asyncapi", "description", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "tags", || ConstraintSemantics::Annotation),
    row("asyncapi", "security", || ConstraintSemantics::Annotation),
    row("asyncapi", "enum", || ConstraintSemantics::Annotation),
    row("asyncapi", "default", || ConstraintSemantics::Annotation),
    row("asyncapi", "examples", || ConstraintSemantics::Annotation),
    row("asyncapi", "contentType", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "name", || ConstraintSemantics::Annotation),
    row("asyncapi", "location", || ConstraintSemantics::Annotation),
    row("asyncapi", "asyncapi", || ConstraintSemantics::Annotation),
    row("asyncapi", "id", || ConstraintSemantics::Annotation),
    row("asyncapi", "servers", || ConstraintSemantics::Annotation),
    row("asyncapi", "defaultContentType", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "channels", || ConstraintSemantics::Annotation),
    row("asyncapi", "operations", || ConstraintSemantics::Annotation),
    row("asyncapi", "version", || ConstraintSemantics::Annotation),
    row("asyncapi", "termsOfService", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "url", || ConstraintSemantics::Annotation),
    row("asyncapi", "email", || ConstraintSemantics::Annotation),
    row("asyncapi", "host", || ConstraintSemantics::Annotation),
    row("asyncapi", "protocol", || ConstraintSemantics::Annotation),
    row("asyncapi", "protocolVersion", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "pathname", || ConstraintSemantics::Annotation),
    row("asyncapi", "variables", || ConstraintSemantics::Annotation),
    row("asyncapi", "address", || ConstraintSemantics::Annotation),
    row("asyncapi", "messages", || ConstraintSemantics::Annotation),
    row("asyncapi", "parameters", || ConstraintSemantics::Annotation),
    row("asyncapi", "action", || ConstraintSemantics::Annotation),
    row("asyncapi", "traits", || ConstraintSemantics::Annotation),
    row("asyncapi", "headers", || ConstraintSemantics::Annotation),
    row("asyncapi", "payload", || ConstraintSemantics::Annotation),
    row("asyncapi", "schemas", || ConstraintSemantics::Annotation),
    row("asyncapi", "securitySchemes", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "serverVariables", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "correlationIds", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "replies", || ConstraintSemantics::Annotation),
    row("asyncapi", "replyAddresses", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "externalDocs", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "operationTraits", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "messageTraits", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "serverBindings", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "channelBindings", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "operationBindings", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "messageBindings", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "schemaFormat", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "type", || ConstraintSemantics::Annotation),
    row("asyncapi", "in", || ConstraintSemantics::Annotation),
    row("asyncapi", "scheme", || ConstraintSemantics::Annotation),
    row("asyncapi", "bearerFormat", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "openIdConnectUrl", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "scopes", || ConstraintSemantics::Annotation),
    row("asyncapi", "authorizationUrl", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "tokenUrl", || ConstraintSemantics::Annotation),
    row("asyncapi", "refreshUrl", || ConstraintSemantics::Annotation),
    row("asyncapi", "availableScopes", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "http", || ConstraintSemantics::Annotation),
    row("asyncapi", "ws", || ConstraintSemantics::Annotation),
    row("asyncapi", "kafka", || ConstraintSemantics::Annotation),
    row("asyncapi", "anypointmq", || ConstraintSemantics::Annotation),
    row("asyncapi", "amqp", || ConstraintSemantics::Annotation),
    row("asyncapi", "amqp1", || ConstraintSemantics::Annotation),
    row("asyncapi", "mqtt", || ConstraintSemantics::Annotation),
    row("asyncapi", "mqtt5", || ConstraintSemantics::Annotation),
    row("asyncapi", "nats", || ConstraintSemantics::Annotation),
    row("asyncapi", "jms", || ConstraintSemantics::Annotation),
    row("asyncapi", "sns", || ConstraintSemantics::Annotation),
    row("asyncapi", "solace", || ConstraintSemantics::Annotation),
    row("asyncapi", "sqs", || ConstraintSemantics::Annotation),
    row("asyncapi", "stomp", || ConstraintSemantics::Annotation),
    row("asyncapi", "redis", || ConstraintSemantics::Annotation),
    row("asyncapi", "mercure", || ConstraintSemantics::Annotation),
    row("asyncapi", "ibmmq", || ConstraintSemantics::Annotation),
    row("asyncapi", "googlepubsub", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "pulsar", || ConstraintSemantics::Annotation),
    row("asyncapi", "ros2", || ConstraintSemantics::Annotation),
    row("asyncapi", "extension", || ConstraintSemantics::Annotation),
    row("asyncapi", "ignored", || ConstraintSemantics::Annotation),
    row("asyncapi", "$comment", || ConstraintSemantics::Annotation),
    row("asyncapi", "const", || ConstraintSemantics::Annotation),
    row("asyncapi", "multipleOf", || ConstraintSemantics::Annotation),
    row("asyncapi", "maximum", || ConstraintSemantics::Maximum),
    row("asyncapi", "exclusiveMaximum", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "minimum", || ConstraintSemantics::Minimum),
    row("asyncapi", "exclusiveMinimum", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("asyncapi", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("asyncapi", "pattern", || ConstraintSemantics::Annotation),
    row("asyncapi", "maxItems", || ConstraintSemantics::Annotation),
    row("asyncapi", "minItems", || ConstraintSemantics::Annotation),
    row("asyncapi", "uniqueItems", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "maxProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "minProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "required", || ConstraintSemantics::Annotation),
    row("asyncapi", "format", || ConstraintSemantics::Annotation),
    row("asyncapi", "contentEncoding", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "contentMediaType", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "readOnly", || ConstraintSemantics::Annotation),
    row("asyncapi", "writeOnly", || ConstraintSemantics::Annotation),
    row("asyncapi", "definitions", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "properties", || ConstraintSemantics::Annotation),
    row("asyncapi", "patternProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "annotation", || ConstraintSemantics::Annotation),
    row("asyncapi", "discriminator", || {
        ConstraintSemantics::Annotation
    }),
    row("asyncapi", "deprecated", || ConstraintSemantics::Annotation),
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
