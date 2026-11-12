//! `OpenAPI` 3.1 protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.
//!
//! An `OpenAPI` description (version 3.1.x) is JSON or YAML, and this crate
//! links no grammar for either: `panproto_io::api::openapi::read_openapi`
//! reads it through the shared JSON and YAML routes of `panproto-io`
//! (feature `tree-sitter`) into a schema of this protocol, and both schema
//! dispatches refuse the protocol with an error naming that reader.
//!
//! Every object of the description is a vertex whose kind names the object
//! ([`OBJECTS`]): the `openapi` root, `info`, `path-item`, `operation`,
//! `parameter` and the rest, and `reference` for a Reference Object. A
//! field that holds objects is an edge kind ([`Field::edge`]) from the
//! object to each object it holds, named by the map key or list index; a
//! field that holds a value is a constraint of the field's name holding the
//! value as canonical JSON text, as is a map or list field that is empty;
//! and a specification extension (`x-`) is an [`EXTENSION`] constraint
//! holding the member as written. Schema Objects are `json-schema`
//! subschemas (dialect 2020-12 with the `OpenAPI` vocabulary,
//! [`VOCABULARY`]), so this protocol declares that protocol's kinds, edges
//! and constraint sorts too. Reference Objects, a Path Item's `$ref`, a
//! Link's operation and a Security Requirement's schemes are edges
//! ([`REF`], [`OPERATION`], [`REQUIREMENT`]) to what they resolve to.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::data_schema::json_schema;
use crate::theories;

/// The dialect id of the `OpenAPI` 3.1 Schema Object (the "OAS dialect
/// schema id"), the default `$schema` of every Schema Object.
pub const OAS_DIALECT: &str = "https://spec.openapis.org/oas/3.1/dialect/base";

/// The keywords the `OpenAPI` vocabulary adds to 2020-12, kept as
/// constraints of a Schema Object under the `OpenAPI` dialect.
pub const VOCABULARY: &[&str] = &["discriminator", "xml", "externalDocs", "example"];

/// The constraint sort of a specification extension: its value is the JSON
/// text of an object holding the one `x-` member as written.
pub const EXTENSION: &str = "extension";

/// The constraint sort of a member of a Reference Object other than
/// `$ref`, `summary` and `description`, which the specification says is
/// ignored: its value is the JSON text of an object holding the member.
pub const IGNORED: &str = "ignored";

/// The constraint sort of a Security Requirement's entry: the JSON text of
/// an object holding the scheme name and its list of scopes or roles.
pub const SCOPES: &str = "scopes";

/// The edge kind of a reference: a Reference Object's `$ref`, a Path
/// Item's `$ref` and a Schema Object's `$ref`, to what it resolves to.
pub const REF: &str = "ref";

/// The edge kind from a Link to the operation its `operationRef` or
/// `operationId` names.
pub const OPERATION: &str = "operation";

/// The edge kind from a Security Requirement to each security scheme it
/// names, named by the scheme's name.
pub const REQUIREMENT: &str = "requirement";

/// The kind of the Reference Object vertex.
pub const REFERENCE: &str = "reference";

/// The operation fields of a Path Item, in the specification's order.
pub const METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// How a field holds its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holds {
    /// A string.
    Str,
    /// A boolean.
    Bool,
    /// Any JSON value.
    Any,
    /// An array of strings.
    Strings,
    /// An object whose values are strings.
    StringMap,
    /// An object whose values are any JSON values.
    AnyMap,
    /// One object of the kind named.
    One(&'static str),
    /// One object of the kind named, or a Reference Object to one.
    OneOrRef(&'static str),
    /// An array of objects of the kind named.
    List(&'static str),
    /// An array of objects of the kind named or Reference Objects to them.
    ListOrRef(&'static str),
    /// An object whose values are objects of the kind named.
    Map(&'static str),
    /// An object whose values are objects of the kind named or Reference
    /// Objects to them.
    MapOrRef(&'static str),
    /// One Schema Object.
    Schema,
    /// An object whose values are Schema Objects.
    SchemaMap,
    /// The patterned fields of an object, whose keys the reader checks
    /// (the paths of a Paths Object, the status codes of a Responses
    /// Object, the expressions of a Callback): an edge per key to an object
    /// of the kind named, or to a Reference Object when `or_ref`.
    Patterned {
        /// The kind of the objects.
        kind: &'static str,
        /// Whether a Reference Object may stand for one.
        or_ref: bool,
    },
}

impl Holds {
    /// Whether the field holds a value, kept as a constraint, rather than
    /// objects reached by edges.
    #[must_use]
    pub const fn is_value(self) -> bool {
        matches!(
            self,
            Self::Str | Self::Bool | Self::Any | Self::Strings | Self::StringMap | Self::AnyMap
        )
    }

    /// Whether the field holds a map or list of objects, kept as a
    /// constraint of the field's name when it is empty.
    #[must_use]
    pub const fn is_collection(self) -> bool {
        matches!(
            self,
            Self::List(_) | Self::ListOrRef(_) | Self::Map(_) | Self::MapOrRef(_) | Self::SchemaMap
        )
    }

    /// The kind of the objects the field holds, if it holds objects of an
    /// `OpenAPI` kind.
    #[must_use]
    pub const fn kind(self) -> Option<&'static str> {
        match self {
            Self::One(k)
            | Self::OneOrRef(k)
            | Self::List(k)
            | Self::ListOrRef(k)
            | Self::Map(k)
            | Self::MapOrRef(k)
            | Self::Patterned { kind: k, .. } => Some(k),
            _ => None,
        }
    }

    /// Whether a Reference Object may stand for an object the field holds.
    #[must_use]
    pub const fn or_ref(self) -> bool {
        matches!(
            self,
            Self::OneOrRef(_)
                | Self::ListOrRef(_)
                | Self::MapOrRef(_)
                | Self::Patterned { or_ref: true, .. }
        )
    }
}

/// One fixed field of an object, or its patterned fields under
/// [`Holds::Patterned`].
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// The field's name, which is also the constraint sort of a value
    /// field and of an empty map or list; empty for patterned fields.
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

const fn patterned(kind: &'static str, or_ref: bool, edge: &'static str) -> Field {
    Field {
        name: "",
        holds: Holds::Patterned { kind, or_ref },
        required: false,
        edge,
    }
}

const EXTERNAL_DOCS: Field = objects("externalDocs", Holds::One("external-docs"), "external-docs");
const SERVERS: Field = objects("servers", Holds::List("server"), "servers");
const SECURITY: Field = objects("security", Holds::List("security-requirement"), "security");
const PARAMETERS: Field = objects("parameters", Holds::ListOrRef("parameter"), "parameters");
const EXAMPLES: Field = objects("examples", Holds::MapOrRef("example"), "examples");
const CONTENT: Field = objects("content", Holds::Map("media-type"), "content");
const HEADERS: Field = objects("headers", Holds::MapOrRef("header"), "headers");
const SCHEMA: Field = objects("schema", Holds::Schema, "schema");
const DESCRIPTION: Field = value("description", Holds::Str);
const SUMMARY: Field = value("summary", Holds::Str);

/// Every object kind of `OpenAPI` 3.1 other than the Schema Object (a
/// `json-schema` subschema) and the Reference Object ([`REFERENCE`]), with
/// its fields in canonical order.
pub const OBJECTS: &[Object] = &[
    Object {
        kind: "openapi",
        fields: &[
            required("openapi", Holds::Str),
            required_objects("info", Holds::One("info"), "info"),
            value("jsonSchemaDialect", Holds::Str),
            SERVERS,
            objects("paths", Holds::One("paths"), "paths"),
            objects("webhooks", Holds::Map("path-item"), "webhooks"),
            objects("components", Holds::One("components"), "components"),
            SECURITY,
            objects("tags", Holds::List("tag"), "tags"),
            EXTERNAL_DOCS,
        ],
    },
    Object {
        kind: "info",
        fields: &[
            required("title", Holds::Str),
            SUMMARY,
            DESCRIPTION,
            value("termsOfService", Holds::Str),
            objects("contact", Holds::One("contact"), "contact"),
            objects("license", Holds::One("license"), "license"),
            required("version", Holds::Str),
        ],
    },
    Object {
        kind: "contact",
        fields: &[
            value("name", Holds::Str),
            value("url", Holds::Str),
            value("email", Holds::Str),
        ],
    },
    Object {
        kind: "license",
        fields: &[
            required("name", Holds::Str),
            value("identifier", Holds::Str),
            value("url", Holds::Str),
        ],
    },
    Object {
        kind: "server",
        fields: &[
            required("url", Holds::Str),
            DESCRIPTION,
            objects("variables", Holds::Map("server-variable"), "variables"),
        ],
    },
    Object {
        kind: "server-variable",
        fields: &[
            value("enum", Holds::Strings),
            required("default", Holds::Str),
            DESCRIPTION,
        ],
    },
    Object {
        kind: "components",
        fields: &[
            objects("schemas", Holds::SchemaMap, "schemas"),
            objects("responses", Holds::MapOrRef("response"), "responses"),
            objects("parameters", Holds::MapOrRef("parameter"), "parameters"),
            EXAMPLES,
            objects(
                "requestBodies",
                Holds::MapOrRef("request-body"),
                "request-bodies",
            ),
            HEADERS,
            objects(
                "securitySchemes",
                Holds::MapOrRef("security-scheme"),
                "security-schemes",
            ),
            objects("links", Holds::MapOrRef("link"), "links"),
            objects("callbacks", Holds::MapOrRef("callback"), "callbacks"),
            objects("pathItems", Holds::Map("path-item"), "path-items"),
        ],
    },
    Object {
        kind: "paths",
        fields: &[patterned("path-item", false, "path")],
    },
    Object {
        kind: "path-item",
        fields: &[
            SUMMARY,
            DESCRIPTION,
            objects("get", Holds::One("operation"), "get"),
            objects("put", Holds::One("operation"), "put"),
            objects("post", Holds::One("operation"), "post"),
            objects("delete", Holds::One("operation"), "delete"),
            objects("options", Holds::One("operation"), "options"),
            objects("head", Holds::One("operation"), "head"),
            objects("patch", Holds::One("operation"), "patch"),
            objects("trace", Holds::One("operation"), "trace"),
            SERVERS,
            PARAMETERS,
        ],
    },
    Object {
        kind: "operation",
        fields: &[
            value("tags", Holds::Strings),
            SUMMARY,
            DESCRIPTION,
            EXTERNAL_DOCS,
            value("operationId", Holds::Str),
            PARAMETERS,
            objects(
                "requestBody",
                Holds::OneOrRef("request-body"),
                "request-body",
            ),
            objects("responses", Holds::One("responses"), "responses"),
            objects("callbacks", Holds::MapOrRef("callback"), "callbacks"),
            value("deprecated", Holds::Bool),
            SECURITY,
            SERVERS,
        ],
    },
    Object {
        kind: "external-docs",
        fields: &[DESCRIPTION, required("url", Holds::Str)],
    },
    Object {
        kind: "parameter",
        fields: &[
            required("name", Holds::Str),
            required("in", Holds::Str),
            DESCRIPTION,
            value("required", Holds::Bool),
            value("deprecated", Holds::Bool),
            value("allowEmptyValue", Holds::Bool),
            value("style", Holds::Str),
            value("explode", Holds::Bool),
            value("allowReserved", Holds::Bool),
            SCHEMA,
            value("example", Holds::Any),
            EXAMPLES,
            CONTENT,
        ],
    },
    Object {
        kind: "request-body",
        fields: &[
            DESCRIPTION,
            required_objects("content", Holds::Map("media-type"), "content"),
            value("required", Holds::Bool),
        ],
    },
    Object {
        kind: "media-type",
        fields: &[
            SCHEMA,
            value("example", Holds::Any),
            EXAMPLES,
            objects("encoding", Holds::Map("encoding"), "encoding"),
        ],
    },
    Object {
        kind: "encoding",
        fields: &[
            value("contentType", Holds::Str),
            HEADERS,
            value("style", Holds::Str),
            value("explode", Holds::Bool),
            value("allowReserved", Holds::Bool),
        ],
    },
    Object {
        kind: "responses",
        fields: &[patterned("response", true, "response")],
    },
    Object {
        kind: "response",
        fields: &[
            required("description", Holds::Str),
            HEADERS,
            CONTENT,
            objects("links", Holds::MapOrRef("link"), "links"),
        ],
    },
    Object {
        kind: "callback",
        fields: &[patterned("path-item", false, "expression")],
    },
    Object {
        kind: "example",
        fields: &[
            SUMMARY,
            DESCRIPTION,
            value("value", Holds::Any),
            value("externalValue", Holds::Str),
        ],
    },
    Object {
        kind: "link",
        fields: &[
            value("operationRef", Holds::Str),
            value("operationId", Holds::Str),
            value("parameters", Holds::AnyMap),
            value("requestBody", Holds::Any),
            DESCRIPTION,
            objects("server", Holds::One("server"), "server"),
        ],
    },
    Object {
        kind: "header",
        fields: &[
            DESCRIPTION,
            value("required", Holds::Bool),
            value("deprecated", Holds::Bool),
            value("style", Holds::Str),
            value("explode", Holds::Bool),
            SCHEMA,
            value("example", Holds::Any),
            EXAMPLES,
            CONTENT,
        ],
    },
    Object {
        kind: "tag",
        fields: &[required("name", Holds::Str), DESCRIPTION, EXTERNAL_DOCS],
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
            required("scopes", Holds::StringMap),
        ],
    },
    Object {
        kind: "security-requirement",
        fields: &[],
    },
];

/// The fields of the Reference Object other than `$ref` (which is the
/// [`REF`] edge), in canonical order.
pub const REFERENCE_FIELDS: &[Field] = &[SUMMARY, DESCRIPTION];

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
/// (an empty one is kept under its name), [`EXTENSION`], [`IGNORED`],
/// [`SCOPES`], and the `json-schema` sorts with the [`VOCABULARY`].
#[must_use]
pub fn constraint_sorts() -> Vec<String> {
    let fields = OBJECTS
        .iter()
        .flat_map(|o| o.fields)
        .chain(REFERENCE_FIELDS)
        .filter(|f| f.holds.is_value() || f.holds.is_collection())
        .map(|f| f.name);
    let rest = [EXTENSION, IGNORED, SCOPES]
        .into_iter()
        .chain(json_schema::KEPT.iter().copied())
        .chain(json_schema::EMPTY_KEPT.iter().copied())
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

/// Returns the `OpenAPI` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "openapi".into(),
        schema_theory: "ThOpenAPISchema".into(),
        instance_theory: "ThOpenAPIInstance".into(),
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

/// Register the component GATs for `OpenAPI` with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThOpenAPISchema",
        "ThOpenAPIInstance",
    );
}

/// The targets of the edges of `field`: the kind it holds (and a Reference
/// Object where one may stand for it), or the subschema kinds.
fn targets(field: &Field) -> Vec<&'static str> {
    match (field.holds, field.holds.kind()) {
        (Holds::Schema | Holds::SchemaMap, _) => json_schema::KINDS.to_vec(),
        (holds, Some(kind)) if holds.or_ref() => vec![kind, REFERENCE],
        (_, Some(kind)) => vec![kind],
        (_, None) => Vec::new(),
    }
}

/// The edge rules: each object field's edge joins the objects that have the
/// field to the kinds it holds; [`OPERATION`] joins a link to an operation,
/// [`REQUIREMENT`] a security requirement to a security scheme or a
/// Reference Object standing for one, and [`REF`] and the `json-schema`
/// edges join any two vertices, since a `ref` edge leaves a Reference
/// Object, a Path Item or a subschema.
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
    rules.push(EdgeRule {
        edge_kind: OPERATION.into(),
        src_kinds: vec!["link".into()],
        tgt_kinds: vec!["operation".into()],
    });
    rules.push(EdgeRule {
        edge_kind: REQUIREMENT.into(),
        src_kinds: vec!["security-requirement".into()],
        tgt_kinds: vec!["security-scheme".into(), REFERENCE.into()],
    });
    for kind in json_schema::APPLICATOR_EDGES
        .iter()
        .map(|(_, kind)| *kind)
        .chain([REF, "dynamic-ref"])
    {
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
        assert_eq!(p.name, "openapi");
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
    fn register_theories_registers_the_schema_and_instance_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThOpenAPISchema"));
        assert!(registry.contains_key("ThOpenAPIInstance"));
    }
}
