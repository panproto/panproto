//! `AsyncAPI` 3 protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.
//!
//! An `AsyncAPI` description (major version 3, read against the 3.1.0
//! text) is JSON or YAML, and this crate links no grammar for either:
//! `panproto_io::api::asyncapi::read_asyncapi` reads it through the shared
//! JSON and YAML routes of `panproto-io` (feature `tree-sitter`) into a
//! schema of this protocol, and both schema dispatches refuse the protocol
//! with an error naming that reader.
//!
//! Every object of the description is a vertex whose kind names the object
//! ([`OBJECTS`](crate::api::asyncapi::OBJECTS)): the `asyncapi` root,
//! `info`, `server`, `channel`, `operation`, `message` and the rest, and
//! `reference` for a Reference Object. A field that holds objects is an
//! edge kind ([`Field::edge`](crate::api::asyncapi::Field::edge)) from the
//! object to each object it holds, named by the map key or list index; a
//! field that holds a value is a constraint of the field's name holding the
//! value as canonical JSON text, as is a map or list field that is empty;
//! and a specification extension (`x-`) is an
//! [`EXTENSION`](crate::api::asyncapi::EXTENSION) constraint holding the
//! member as written. A binding (the value of one protocol's field of a
//! bindings object) is a value field, kept as its JSON text. Schema Objects
//! are `json-schema` subschemas read as JSON Schema draft-07 with the
//! `AsyncAPI` vocabulary ([`VOCABULARY`](crate::api::asyncapi::VOCABULARY)),
//! so this protocol declares the draft-07 part of that protocol's kinds,
//! edges and constraint sorts ([`SCHEMA_KEPT`](crate::api::asyncapi::SCHEMA_KEPT),
//! [`SCHEMA_EDGES`](crate::api::asyncapi::SCHEMA_EDGES)) too. A Reference
//! Object is a vertex with a [`REF`](crate::api::asyncapi::REF) edge to
//! what it resolves to.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::data_schema::json_schema;
use crate::theories;

/// The keywords the `AsyncAPI` vocabulary adds to draft-07, kept as
/// constraints of a Schema Object read in the `AsyncAPI` schema format.
pub const VOCABULARY: &[&str] = &["discriminator", "externalDocs", "deprecated"];

/// The constraint sort of a specification extension: its value is the JSON
/// text of an object holding the one `x-` member as written.
pub const EXTENSION: &str = "extension";

/// The constraint sort of a member of a Reference Object other than
/// `$ref`, which the specification says is ignored: its value is the JSON
/// text of an object holding the member.
pub const IGNORED: &str = "ignored";

/// The edge kind from a Reference Object to what its `$ref` resolves to,
/// named by the reference as written.
pub const REF: &str = "ref";

/// The kind of the Reference Object vertex.
pub const REFERENCE: &str = "reference";

/// The kind a place holding a Schema Object takes: a reference from such a
/// place names a Schema Object or a Multi Format Schema Object.
pub const SCHEMA: &str = "schema";

/// The kind of the Multi Format Schema Object vertex.
pub const MULTI_FORMAT_SCHEMA: &str = "multi-format-schema";

/// The `json-schema` keywords kept as constraints of a draft-07 Schema
/// Object.
///
/// They are the draft-07 keywords that are neither references nor
/// applicators, without `$schema` and `$id`, which the reader refuses since
/// `AsyncAPI` resolves `$ref` as a JSON Reference.
pub const SCHEMA_KEPT: &[&str] = &[
    "$comment",
    "type",
    "enum",
    "const",
    "multipleOf",
    "maximum",
    "exclusiveMaximum",
    "minimum",
    "exclusiveMinimum",
    "maxLength",
    "minLength",
    "pattern",
    "maxItems",
    "minItems",
    "uniqueItems",
    "maxProperties",
    "minProperties",
    "required",
    "format",
    "contentEncoding",
    "contentMediaType",
    "title",
    "description",
    "default",
    "readOnly",
    "writeOnly",
    "examples",
];

/// The draft-07 keywords holding an object of subschemas, kept as a
/// constraint of their name when the object is empty.
pub const SCHEMA_EMPTY_KEPT: &[&str] = &["definitions", "properties", "patternProperties"];

/// The edge kind of each draft-07 applicator keyword (and `definitions`).
pub const SCHEMA_EDGES: &[(&str, &str)] = &[
    json_schema::DRAFT_07_DEFINITIONS,
    ("properties", "prop"),
    ("patternProperties", "pattern-prop"),
    ("additionalProperties", "additional-props"),
    ("propertyNames", "property-names"),
    ("items", "items"),
    ("contains", "contains"),
    ("allOf", "all-of"),
    ("anyOf", "any-of"),
    ("oneOf", "one-of"),
    ("not", "not"),
    ("if", "if"),
    ("then", "then"),
    ("else", "else"),
];

/// The protocol fields of a Server Bindings, Channel Bindings and Message
/// Bindings Object, in the order of the specification's tables.
pub const BINDINGS: [&str; 20] = [
    "http",
    "ws",
    "kafka",
    "anypointmq",
    "amqp",
    "amqp1",
    "mqtt",
    "mqtt5",
    "nats",
    "jms",
    "sns",
    "solace",
    "sqs",
    "stomp",
    "redis",
    "mercure",
    "ibmmq",
    "googlepubsub",
    "pulsar",
    "ros2",
];

/// The protocol fields of an Operation Bindings Object, in the order of
/// the specification's table, which names `googlepubsub` before `ibmmq`.
pub const OPERATION_BINDINGS: [&str; 20] = [
    "http",
    "ws",
    "kafka",
    "anypointmq",
    "amqp",
    "amqp1",
    "mqtt",
    "mqtt5",
    "nats",
    "jms",
    "sns",
    "solace",
    "sqs",
    "stomp",
    "redis",
    "mercure",
    "googlepubsub",
    "ibmmq",
    "pulsar",
    "ros2",
];

/// How a field holds its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holds {
    /// A string.
    Str,
    /// A string or `null`.
    StrOrNull,
    /// Any JSON value.
    Any,
    /// An array of strings.
    Strings,
    /// An object whose values are strings.
    StringMap,
    /// An object, with any values.
    AnyMap,
    /// One object of the kind named.
    One(&'static str),
    /// One object of the kind named, or a Reference Object to one.
    OneOrRef(&'static str),
    /// A Reference Object to an object of the kind named, never the object.
    Ref(&'static str),
    /// An array of Reference Objects to objects of the kind named.
    Refs(&'static str),
    /// An array of objects of the kind named.
    List(&'static str),
    /// An array of objects of the kind named or Reference Objects to them.
    ListOrRef(&'static str),
    /// An object whose values are objects of the kind named or Reference
    /// Objects to them.
    MapOrRef(&'static str),
    /// A Schema Object, a Multi Format Schema Object, or a Reference Object
    /// to either.
    Schema,
    /// An object whose values are as [`Holds::Schema`].
    SchemaMap,
    /// The `schema` of a Multi Format Schema Object: a Schema Object of the
    /// object's schema format.
    FormatSchema,
}

impl Holds {
    /// Whether the field holds a value, kept as a constraint, rather than
    /// objects reached by edges.
    #[must_use]
    pub const fn is_value(self) -> bool {
        matches!(
            self,
            Self::Str
                | Self::StrOrNull
                | Self::Any
                | Self::Strings
                | Self::StringMap
                | Self::AnyMap
        )
    }

    /// Whether the field holds a map or list of objects, kept as a
    /// constraint of the field's name when it is empty.
    #[must_use]
    pub const fn is_collection(self) -> bool {
        matches!(
            self,
            Self::Refs(_)
                | Self::List(_)
                | Self::ListOrRef(_)
                | Self::MapOrRef(_)
                | Self::SchemaMap
        )
    }

    /// The kind of the objects the field holds, if it holds objects of an
    /// `AsyncAPI` kind.
    #[must_use]
    pub const fn kind(self) -> Option<&'static str> {
        match self {
            Self::One(k)
            | Self::OneOrRef(k)
            | Self::Ref(k)
            | Self::Refs(k)
            | Self::List(k)
            | Self::ListOrRef(k)
            | Self::MapOrRef(k) => Some(k),
            _ => None,
        }
    }
}

/// One fixed field of an object.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// The field's name, which is also the constraint sort of a value
    /// field and of an empty map or list.
    pub name: &'static str,
    /// How the field holds its value.
    pub holds: Holds,
    /// Whether the specification says the field is REQUIRED in every
    /// object of the kind.
    pub required: bool,
    /// The edge kind from the object to each object the field holds, empty
    /// for a value field.
    pub edge: &'static str,
}

/// One kind of object: its vertex kind and its fields in canonical order,
/// the order of the specification's tables.
#[derive(Clone, Copy, Debug)]
pub struct Object {
    /// The vertex kind.
    pub kind: &'static str,
    /// The fields.
    pub fields: &'static [Field],
}

const fn value(name: &'static str, holds: Holds) -> Field {
    Field {
        name,
        holds,
        required: false,
        edge: "",
    }
}

const fn required(name: &'static str, holds: Holds) -> Field {
    Field {
        name,
        holds,
        required: true,
        edge: "",
    }
}

const fn objects(name: &'static str, holds: Holds, edge: &'static str) -> Field {
    Field {
        name,
        holds,
        required: false,
        edge,
    }
}

const fn required_objects(name: &'static str, holds: Holds, edge: &'static str) -> Field {
    Field {
        name,
        holds,
        required: true,
        edge,
    }
}

const TITLE: Field = value("title", Holds::Str);
const SUMMARY: Field = value("summary", Holds::Str);
const DESCRIPTION: Field = value("description", Holds::Str);
const TAGS: Field = objects("tags", Holds::ListOrRef("tag"), "tags");
const EXTERNAL_DOCS: Field = objects(
    "externalDocs",
    Holds::OneOrRef("external-docs"),
    "external-docs",
);
const SECURITY: Field = objects("security", Holds::ListOrRef("security-scheme"), "security");
const ENUM: Field = value("enum", Holds::Strings);
const DEFAULT: Field = value("default", Holds::Str);
const EXAMPLES: Field = value("examples", Holds::Strings);
const CORRELATION_ID: Field = objects(
    "correlationId",
    Holds::OneOrRef("correlation-id"),
    "correlation-id",
);
const CONTENT_TYPE: Field = value("contentType", Holds::Str);
const NAME: Field = value("name", Holds::Str);
const MESSAGE_EXAMPLES: Field = objects("examples", Holds::List("message-example"), "examples");
const HEADERS: Field = objects("headers", Holds::Schema, "headers");
const MESSAGE_BINDINGS: Field =
    objects("bindings", Holds::OneOrRef("message-bindings"), "bindings");
const OPERATION_BINDINGS_FIELD: Field = objects(
    "bindings",
    Holds::OneOrRef("operation-bindings"),
    "bindings",
);
const LOCATION: Field = required("location", Holds::Str);

/// The binding fields of a bindings object, one per protocol of `names`.
const fn bindings<const N: usize>(names: [&'static str; N]) -> [Field; N] {
    let mut fields = [value("", Holds::AnyMap); N];
    let mut i = 0;
    while i < N {
        fields[i] = value(names[i], Holds::AnyMap);
        i += 1;
    }
    fields
}

const BINDING_FIELDS: [Field; 20] = bindings(BINDINGS);

const OPERATION_BINDING_FIELDS: [Field; 20] = bindings(OPERATION_BINDINGS);

/// Every object kind of `AsyncAPI` 3 other than the Schema Object (a
/// `json-schema` subschema) and the Reference Object ([`REFERENCE`]), with
/// its fields in canonical order.
pub const OBJECTS: &[Object] = &[
    Object {
        kind: "asyncapi",
        fields: &[
            required("asyncapi", Holds::Str),
            value("id", Holds::Str),
            required_objects("info", Holds::One("info"), "info"),
            objects("servers", Holds::MapOrRef("server"), "servers"),
            value("defaultContentType", Holds::Str),
            objects("channels", Holds::MapOrRef("channel"), "channels"),
            objects("operations", Holds::MapOrRef("operation"), "operations"),
            objects("components", Holds::One("components"), "components"),
        ],
    },
    Object {
        kind: "info",
        fields: &[
            required("title", Holds::Str),
            required("version", Holds::Str),
            DESCRIPTION,
            value("termsOfService", Holds::Str),
            objects("contact", Holds::One("contact"), "contact"),
            objects("license", Holds::One("license"), "license"),
            TAGS,
            EXTERNAL_DOCS,
        ],
    },
    Object {
        kind: "contact",
        fields: &[NAME, value("url", Holds::Str), value("email", Holds::Str)],
    },
    Object {
        kind: "license",
        fields: &[required("name", Holds::Str), value("url", Holds::Str)],
    },
    Object {
        kind: "server",
        fields: &[
            required("host", Holds::Str),
            required("protocol", Holds::Str),
            value("protocolVersion", Holds::Str),
            value("pathname", Holds::Str),
            DESCRIPTION,
            TITLE,
            SUMMARY,
            objects("variables", Holds::MapOrRef("server-variable"), "variables"),
            SECURITY,
            TAGS,
            EXTERNAL_DOCS,
            objects("bindings", Holds::OneOrRef("server-bindings"), "bindings"),
        ],
    },
    Object {
        kind: "server-variable",
        fields: &[ENUM, DEFAULT, DESCRIPTION, EXAMPLES],
    },
    Object {
        kind: "channel",
        fields: &[
            value("address", Holds::StrOrNull),
            objects("messages", Holds::MapOrRef("message"), "messages"),
            TITLE,
            SUMMARY,
            DESCRIPTION,
            objects("servers", Holds::Refs("server"), "servers"),
            objects("parameters", Holds::MapOrRef("parameter"), "parameters"),
            TAGS,
            EXTERNAL_DOCS,
            objects("bindings", Holds::OneOrRef("channel-bindings"), "bindings"),
        ],
    },
    Object {
        kind: "operation",
        fields: &[
            required("action", Holds::Str),
            required_objects("channel", Holds::Ref("channel"), "channel"),
            TITLE,
            SUMMARY,
            DESCRIPTION,
            SECURITY,
            TAGS,
            EXTERNAL_DOCS,
            OPERATION_BINDINGS_FIELD,
            objects("traits", Holds::ListOrRef("operation-trait"), "traits"),
            objects("messages", Holds::Refs("message"), "messages"),
            objects("reply", Holds::OneOrRef("operation-reply"), "reply"),
        ],
    },
    Object {
        kind: "operation-trait",
        fields: &[
            TITLE,
            SUMMARY,
            DESCRIPTION,
            SECURITY,
            TAGS,
            EXTERNAL_DOCS,
            OPERATION_BINDINGS_FIELD,
        ],
    },
    Object {
        kind: "operation-reply",
        fields: &[
            objects(
                "address",
                Holds::OneOrRef("operation-reply-address"),
                "address",
            ),
            objects("channel", Holds::Ref("channel"), "channel"),
            objects("messages", Holds::Refs("message"), "messages"),
        ],
    },
    Object {
        kind: "operation-reply-address",
        fields: &[DESCRIPTION, LOCATION],
    },
    Object {
        kind: "parameter",
        fields: &[
            ENUM,
            DEFAULT,
            DESCRIPTION,
            EXAMPLES,
            value("location", Holds::Str),
        ],
    },
    Object {
        kind: "server-bindings",
        fields: &BINDING_FIELDS,
    },
    Object {
        kind: "channel-bindings",
        fields: &BINDING_FIELDS,
    },
    Object {
        kind: "operation-bindings",
        fields: &OPERATION_BINDING_FIELDS,
    },
    Object {
        kind: "message-bindings",
        fields: &BINDING_FIELDS,
    },
    Object {
        kind: "message",
        fields: &[
            HEADERS,
            objects("payload", Holds::Schema, "payload"),
            CORRELATION_ID,
            CONTENT_TYPE,
            NAME,
            TITLE,
            SUMMARY,
            DESCRIPTION,
            TAGS,
            EXTERNAL_DOCS,
            MESSAGE_BINDINGS,
            MESSAGE_EXAMPLES,
            objects("traits", Holds::ListOrRef("message-trait"), "traits"),
        ],
    },
    Object {
        kind: "message-trait",
        fields: &[
            HEADERS,
            CORRELATION_ID,
            CONTENT_TYPE,
            NAME,
            TITLE,
            SUMMARY,
            DESCRIPTION,
            TAGS,
            EXTERNAL_DOCS,
            MESSAGE_BINDINGS,
            MESSAGE_EXAMPLES,
        ],
    },
    Object {
        kind: "message-example",
        fields: &[
            value("headers", Holds::AnyMap),
            value("payload", Holds::Any),
            NAME,
            SUMMARY,
        ],
    },
    Object {
        kind: "tag",
        fields: &[required("name", Holds::Str), DESCRIPTION, EXTERNAL_DOCS],
    },
    Object {
        kind: "external-docs",
        fields: &[DESCRIPTION, required("url", Holds::Str)],
    },
    Object {
        kind: "components",
        fields: &[
            objects("schemas", Holds::SchemaMap, "schemas"),
            objects("servers", Holds::MapOrRef("server"), "servers"),
            objects("channels", Holds::MapOrRef("channel"), "channels"),
            objects("operations", Holds::MapOrRef("operation"), "operations"),
            objects("messages", Holds::MapOrRef("message"), "messages"),
            objects(
                "securitySchemes",
                Holds::MapOrRef("security-scheme"),
                "security-schemes",
            ),
            objects(
                "serverVariables",
                Holds::MapOrRef("server-variable"),
                "server-variables",
            ),
            objects("parameters", Holds::MapOrRef("parameter"), "parameters"),
            objects(
                "correlationIds",
                Holds::MapOrRef("correlation-id"),
                "correlation-ids",
            ),
            objects("replies", Holds::MapOrRef("operation-reply"), "replies"),
            objects(
                "replyAddresses",
                Holds::MapOrRef("operation-reply-address"),
                "reply-addresses",
            ),
            objects(
                "externalDocs",
                Holds::MapOrRef("external-docs"),
                "external-docs",
            ),
            objects("tags", Holds::MapOrRef("tag"), "tags"),
            objects(
                "operationTraits",
                Holds::MapOrRef("operation-trait"),
                "operation-traits",
            ),
            objects(
                "messageTraits",
                Holds::MapOrRef("message-trait"),
                "message-traits",
            ),
            objects(
                "serverBindings",
                Holds::MapOrRef("server-bindings"),
                "server-bindings",
            ),
            objects(
                "channelBindings",
                Holds::MapOrRef("channel-bindings"),
                "channel-bindings",
            ),
            objects(
                "operationBindings",
                Holds::MapOrRef("operation-bindings"),
                "operation-bindings",
            ),
            objects(
                "messageBindings",
                Holds::MapOrRef("message-bindings"),
                "message-bindings",
            ),
        ],
    },
    Object {
        kind: MULTI_FORMAT_SCHEMA,
        fields: &[
            required("schemaFormat", Holds::Str),
            required_objects("schema", Holds::FormatSchema, "schema"),
        ],
    },
    Object {
        kind: "security-scheme",
        fields: &[
            required("type", Holds::Str),
            DESCRIPTION,
            value("name", Holds::Str),
            value("in", Holds::Str),
            value("scheme", Holds::Str),
            value("bearerFormat", Holds::Str),
            objects("flows", Holds::One("oauth-flows"), "flows"),
            value("openIdConnectUrl", Holds::Str),
            value("scopes", Holds::Strings),
        ],
    },
    Object {
        kind: "oauth-flows",
        fields: &[
            objects("implicit", Holds::One("oauth-flow"), "implicit"),
            objects("password", Holds::One("oauth-flow"), "password"),
            objects(
                "clientCredentials",
                Holds::One("oauth-flow"),
                "client-credentials",
            ),
            objects(
                "authorizationCode",
                Holds::One("oauth-flow"),
                "authorization-code",
            ),
        ],
    },
    Object {
        kind: "oauth-flow",
        fields: &[
            value("authorizationUrl", Holds::Str),
            value("tokenUrl", Holds::Str),
            value("refreshUrl", Holds::Str),
            required("availableScopes", Holds::StringMap),
        ],
    },
    Object {
        kind: "correlation-id",
        fields: &[DESCRIPTION, LOCATION],
    },
];

/// The object kind named `kind`.
#[must_use]
pub fn object(kind: &str) -> Option<&'static Object> {
    OBJECTS.iter().find(|o| o.kind == kind)
}

/// Every vertex kind: the object kinds, [`REFERENCE`], and the
/// `json-schema` subschema kinds.
#[must_use]
pub fn kinds() -> Vec<String> {
    OBJECTS
        .iter()
        .map(|o| o.kind)
        .chain([REFERENCE])
        .chain(json_schema::KINDS.iter().copied())
        .map(str::to_owned)
        .collect()
}

/// Every constraint sort, without repeats.
///
/// They are the value fields and the map and list fields of the objects
/// (an empty one is kept under its name), [`EXTENSION`], [`IGNORED`], and
/// the draft-07 sorts of a Schema Object with the [`VOCABULARY`].
#[must_use]
pub fn constraint_sorts() -> Vec<String> {
    let fields = OBJECTS
        .iter()
        .flat_map(|o| o.fields)
        .filter(|f| f.holds.is_value() || f.holds.is_collection())
        .map(|f| f.name);
    let rest = [EXTENSION, IGNORED]
        .into_iter()
        .chain(SCHEMA_KEPT.iter().copied())
        .chain(SCHEMA_EMPTY_KEPT.iter().copied())
        .chain([json_schema::ANNOTATION])
        .chain(VOCABULARY.iter().copied());
    let mut sorts: Vec<String> = Vec::new();
    for sort in fields.chain(rest) {
        if !sorts.iter().any(|s| s == sort) {
            sorts.push(sort.to_owned());
        }
    }
    sorts
}

/// Returns the `AsyncAPI` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "asyncapi".into(),
        schema_theory: "ThAsyncAPISchema".into(),
        instance_theory: "ThAsyncAPIInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: kinds(),
        constraint_sorts: constraint_sorts(),
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for `AsyncAPI` with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThAsyncAPISchema",
        "ThAsyncAPIInstance",
    );
}

/// The targets of the edges of `field`: the kind it holds and, where a
/// Reference Object may stand for one, [`REFERENCE`]; the subschema kinds,
/// [`MULTI_FORMAT_SCHEMA`] and [`REFERENCE`] for a schema field; the
/// subschema kinds for the schema of a Multi Format Schema Object.
fn targets(field: &Field) -> Vec<&'static str> {
    match field.holds {
        Holds::Schema | Holds::SchemaMap => json_schema::KINDS
            .iter()
            .copied()
            .chain([MULTI_FORMAT_SCHEMA, REFERENCE])
            .collect(),
        Holds::FormatSchema => json_schema::KINDS.to_vec(),
        Holds::Ref(_) | Holds::Refs(_) => vec![REFERENCE],
        Holds::OneOrRef(kind) | Holds::ListOrRef(kind) | Holds::MapOrRef(kind) => {
            vec![kind, REFERENCE]
        }
        Holds::One(kind) | Holds::List(kind) => vec![kind],
        _ => Vec::new(),
    }
}

/// The edge rules: each object field's edge joins the objects that have the
/// field to the kinds it holds; [`REF`] and the draft-07 subschema edges
/// join any two vertices, since a `ref` edge leaves a Reference Object or a
/// subschema.
fn edge_rules() -> Vec<EdgeRule> {
    let mut rules: Vec<EdgeRule> = Vec::new();
    for object in OBJECTS {
        for field in object.fields.iter().filter(|f| !f.edge.is_empty()) {
            let index = if let Some(i) = rules.iter().position(|r| r.edge_kind == field.edge) {
                i
            } else {
                rules.push(EdgeRule {
                    edge_kind: field.edge.into(),
                    src_kinds: Vec::new(),
                    tgt_kinds: Vec::new(),
                });
                rules.len() - 1
            };
            let rule = &mut rules[index];
            if !rule.src_kinds.iter().any(|k| k == object.kind) {
                rule.src_kinds.push(object.kind.into());
            }
            for target in targets(field) {
                if !rule.tgt_kinds.iter().any(|k| k == target) {
                    rule.tgt_kinds.push(target.into());
                }
            }
        }
    }
    for kind in SCHEMA_EDGES.iter().map(|(_, kind)| *kind).chain([REF]) {
        rules.push(EdgeRule {
            edge_kind: kind.into(),
            src_kinds: Vec::new(),
            tgt_kinds: Vec::new(),
        });
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_object_kind_and_the_subschema_kinds_are_declared() {
        let p = protocol();
        assert_eq!(p.name, "asyncapi");
        assert_eq!(
            p.obj_kinds.len(),
            OBJECTS.len() + 1 + json_schema::KINDS.len()
        );
        for object in OBJECTS {
            assert!(p.obj_kinds.iter().any(|k| k == object.kind));
        }
    }

    #[test]
    fn edge_kinds_are_declared_once_each() {
        let p = protocol();
        let mut kinds: Vec<&str> = p.edge_rules.iter().map(|r| r.edge_kind.as_str()).collect();
        let count = kinds.len();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(kinds.len(), count);
    }

    #[test]
    fn constraint_sorts_are_declared_once_each_and_cover_the_value_fields() {
        let sorts = constraint_sorts();
        let mut unique = sorts.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), sorts.len());
        for field in OBJECTS.iter().flat_map(|o| o.fields) {
            if field.holds.is_value() {
                assert!(sorts.iter().any(|s| s == field.name), "{}", field.name);
            }
        }
    }

    #[test]
    fn the_binding_fields_are_the_protocol_names_in_table_order() {
        let names = |kind: &str| -> Vec<&str> {
            object(kind)
                .map(|o| o.fields.iter().map(|f| f.name).collect())
                .unwrap_or_default()
        };
        assert_eq!(names("server-bindings"), BINDINGS);
        assert_eq!(names("channel-bindings"), BINDINGS);
        assert_eq!(names("message-bindings"), BINDINGS);
        assert_eq!(names("operation-bindings"), OPERATION_BINDINGS);
    }

    #[test]
    fn the_schema_sorts_and_edges_are_draft_07_keywords_of_json_schema() {
        for keyword in SCHEMA_KEPT {
            assert!(json_schema::KEPT.contains(keyword), "{keyword}");
            assert!(!json_schema::NOT_IN_DRAFT_07.contains(keyword), "{keyword}");
        }
        for (keyword, edge) in SCHEMA_EDGES.iter().skip(1) {
            assert!(
                json_schema::APPLICATOR_EDGES.contains(&(*keyword, *edge)),
                "{keyword}"
            );
        }
    }

    #[test]
    fn register_theories_registers_the_schema_and_instance_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThAsyncAPISchema"));
        assert!(registry.contains_key("ThAsyncAPIInstance"));
    }
}
