//! The `OpenAPI` 3.1 reader and its canonical writer.
//!
//! [`read_openapi`] reads one `OpenAPI` document, and [`read_openapi_bundle`]
//! a description split over several documents, through the shared JSON
//! route ([`crate::config::tree::read_json`], for a document whose first
//! byte other than whitespace is `{`) or the shared YAML route
//! ([`crate::config::tree::read_yaml`], for any other) into a schema of the
//! `openapi` protocol, and refuses with an [`OpenApiError`] naming the
//! document, the JSON pointer and the byte offset of the value at fault
//! every description the reader does not carry exactly. [`write_openapi`]
//! writes one document of such a schema back as canonical JSON.
//!
//! The reader reads version 3.1 of the specification (`openapi` is `3.1.0`,
//! `3.1.1`, `3.1.2` or another `3.1` patch); a Swagger 2.0 document, a 3.0
//! document and a 3.2 document are refused by name. Every object is a
//! vertex whose id is the document's prefix (its retrieval URI, or nothing),
//! `#` and the object's JSON pointer, so `#/paths/~1pets/get` is the `get`
//! operation of the path `/pets`. The protocol's object table
//! ([`panproto_protocols::api::openapi::OBJECTS`]) says how each field is
//! read: a value field becomes a constraint of the field's name holding the
//! value as canonical JSON text, a field holding objects an edge to each.
//! Schema Objects are read by the JSON Schema 2020-12 reader
//! ([`crate::data_schema::json_schema`]) under the `OpenAPI` dialect, so
//! a `$ref` in a Schema Object resolves as JSON Schema resolves it, against
//! every Schema Object of the description. Reference Objects, a Path Item's
//! `$ref`, a Link's operation and a Security Requirement's schemes resolve
//! while the schema is built, to edges; a reference to a document outside
//! the bundle is refused, since the reader fetches nothing.

use std::collections::{HashMap, HashSet};
use std::fmt;

use panproto_protocols::api::openapi::{
    self as protocol, EXTENSION, Field, Holds, IGNORED, METHODS, OAS_DIALECT, REF, REFERENCE,
    REFERENCE_FIELDS, REQUIREMENT, SCOPES, VOCABULARY,
};
use panproto_protocols::data_schema::json_schema::{DIALECT, KINDS as SUBSCHEMA_KINDS};
use panproto_schema::{Schema, SchemaBuilder};

use crate::config::tree::{Node, json_string, read_json, read_yaml};
use crate::data_schema::json_schema::{
    Dialect, Embedded, Host, JsonSchemaErrorKind, JsonSchemaWriteError, Vocabulary, build_embedded,
    pointer_token, write_vertex,
};
use crate::resolution::{resolve_uri_against, resolve_uri_reference};

/// A refusal of the `OpenAPI` reader: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenApiError {
    /// The index of the document at fault in the bundle, `0` for a single
    /// document.
    pub document: usize,
    /// The JSON pointer of the value at fault from the document root, empty
    /// for the root.
    pub path: String,
    /// The byte offset at which the value starts in its document.
    pub offset: usize,
    /// What is wrong.
    pub kind: OpenApiErrorKind,
}

impl fmt::Display for OpenApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if self.path.is_empty() {
            "the root"
        } else {
            &self.path
        };
        write!(
            f,
            "document {}, {path} (byte {}): {}",
            self.document, self.offset, self.kind
        )
    }
}

impl std::error::Error for OpenApiError {}

/// What an [`OpenApiError`] refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OpenApiErrorKind {
    /// The shared JSON or YAML route refuses the document.
    #[error("the {route} route refuses the document: {message}")]
    Syntax {
        /// `JSON` or `YAML`.
        route: &'static str,
        /// The route's refusal.
        message: String,
    },
    /// A Swagger 2.0 document, which the reader does not read.
    #[error("swagger is \"{0}\": Swagger 2.0 documents are not read; only OpenAPI 3.1 is")]
    Swagger(String),
    /// An `openapi` version other than 3.1.
    #[error("openapi is \"{0}\": only OpenAPI 3.1 (3.1.0, 3.1.1, 3.1.2) is read")]
    Version(String),
    /// A document with neither an `openapi` nor a `swagger` member.
    #[error("not an OpenAPI document: the openapi field is missing")]
    NotOpenApi,
    /// A value of another JSON kind than the field takes.
    #[error("expected {0}")]
    WrongType(&'static str),
    /// A REQUIRED field that is absent.
    #[error("the required field {0} is missing")]
    Missing(&'static str),
    /// A field the object does not have.
    #[error("{0} is not a field of this object")]
    UnknownField(String),
    /// A value that breaks a rule of the specification, which the message
    /// states.
    #[error("{0}")]
    Invalid(&'static str),
    /// Two fields the specification makes mutually exclusive.
    #[error("{0} and {1} are mutually exclusive")]
    Exclusive(&'static str, &'static str),
    /// `jsonSchemaDialect` names a dialect the reader does not read.
    #[error("jsonSchemaDialect is \"{0}\"; only the OAS dialect and JSON Schema 2020-12 are read")]
    Dialect(String),
    /// A component name outside `^[a-zA-Z0-9.\-_]+$`.
    #[error("the component name \"{0}\" is not of the form ^[a-zA-Z0-9.-_]+$")]
    ComponentName(String),
    /// A reference to a document outside the bundle.
    #[error("\"{0}\" names a document outside the bundle, and the reader fetches nothing")]
    External(String),
    /// A reference whose target is no object of the description.
    #[error("\"{0}\" names no object of the description")]
    RefTarget(String),
    /// A reference to an object of another kind than its place takes.
    #[error("\"{reference}\" names a {found}, where a {expected} is expected")]
    RefKind {
        /// The reference.
        reference: String,
        /// The kind its place takes.
        expected: &'static str,
        /// The kind it names.
        found: &'static str,
    },
    /// A chain of references that returns to its start.
    #[error("\"{0}\" is part of a cycle of references")]
    RefCycle(String),
    /// Two operations with the same `operationId`.
    #[error("the operationId \"{0}\" is not unique")]
    DuplicateOperationId(String),
    /// A link's `operationId` that no operation has.
    #[error("no operation has the operationId \"{0}\"")]
    UnknownOperationId(String),
    /// A security requirement naming no declared security scheme.
    #[error("\"{0}\" is not a security scheme of the components")]
    UnknownScheme(String),
    /// Two parameters of one list with the same name and location.
    #[error("the parameter {name} in {location} is listed twice")]
    DuplicateParameter {
        /// The parameter's name.
        name: String,
        /// Its location.
        location: String,
    },
    /// A template expression of a path that an operation has no path
    /// parameter for.
    #[error("the path template expression {{{0}}} has no path parameter")]
    MissingPathParameter(String),
    /// A path parameter whose name is no template expression of its path.
    #[error("the path parameter {0} is not a template expression of the path")]
    UnknownPathParameter(String),
    /// A path that differs from another only in its template names.
    #[error("the path {0} is identical to another but for its template names")]
    DuplicatePath(String),
    /// A tag declared twice.
    #[error("the tag \"{0}\" is declared twice")]
    DuplicateTag(String),
    /// A path whose template braces do not pair.
    #[error("the path {0} has an unbalanced or empty template expression")]
    Template(String),
    /// A value the canonical JSON text cannot carry.
    #[error("{0} is not supported")]
    Unsupported(&'static str),
    /// Two documents of the bundle with the same retrieval URI, or two
    /// without one.
    #[error("the document \"{0}\" is given twice")]
    DuplicateDocument(String),
    /// The JSON Schema reader refuses a Schema Object.
    #[error("Schema Object: {0}")]
    Schema(JsonSchemaErrorKind),
    /// The schema the documents describe cannot be built.
    #[error("the schema cannot be built: {0}")]
    Build(String),
}

/// What a document of a bundle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    /// A complete `OpenAPI` document, with an `openapi` field.
    OpenApi,
    /// A JSON Schema document, its root one Schema Object.
    Schema,
}

/// One document of a bundle: its retrieval URI, if it has one, its bytes
/// and what it is.
#[derive(Clone, Copy, Debug)]
pub struct ApiDocument<'a> {
    /// The absolute URI the document was retrieved from, its base URI.
    pub retrieval_uri: Option<&'a str>,
    /// The JSON or YAML text.
    pub source: &'a [u8],
    /// What the document is.
    pub kind: DocumentKind,
}

/// What the reader reads: the schema, and the vertex of each document's
/// root, in document order.
#[derive(Clone, Debug)]
pub struct OpenApiReading {
    /// The schema of the `openapi` protocol.
    pub schema: Schema,
    /// The root vertex of each document.
    pub roots: Vec<String>,
}

type R<T> = Result<T, OpenApiError>;

/// Read `source`, one `OpenAPI` 3.1 document in JSON or YAML with no
/// retrieval URI, into an `openapi` schema.
///
/// # Errors
///
/// Returns an [`OpenApiError`] naming the place at fault.
pub fn read_openapi(source: &[u8]) -> Result<OpenApiReading, OpenApiError> {
    read_openapi_bundle(&[ApiDocument {
        retrieval_uri: None,
        source,
        kind: DocumentKind::OpenApi,
    }])
}

/// Read the documents of an `OpenAPI` description, the first its entry
/// document, into one `openapi` schema, resolving references between them.
///
/// The order of the checks, and so which refusal comes first, is the one
/// the book's `OpenAPI` page states: each document's route and version, the
/// objects of every document in document order, the references, the Schema
/// Objects, then the rules that join objects (operation ids, links,
/// security requirements, parameters and path templates, tags).
///
/// # Errors
///
/// Returns an [`OpenApiError`] naming the place at fault.
pub fn read_openapi_bundle(documents: &[ApiDocument<'_>]) -> Result<OpenApiReading, OpenApiError> {
    let mut trees = Vec::with_capacity(documents.len());
    for (i, document) in documents.iter().enumerate() {
        trees.push(read_tree(i, document.source)?);
    }
    let mut reader = Reader::new(documents, &trees)?;
    for i in 0..trees.len() {
        reader.document = i;
        reader.read_document()?;
    }
    reader.resolve_references()?;
    reader.read_schemas()?;
    reader.check_operations()?;
    reader.check_links()?;
    reader.check_requirements()?;
    reader.check_parameter_lists()?;
    reader.check_paths()?;
    reader.finish()
}

/// Read one document through the route its first byte selects.
fn read_tree(document: usize, source: &[u8]) -> R<Node> {
    let json = source
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{');
    let (route, tree) = if json {
        ("JSON", read_json(source))
    } else {
        ("YAML", read_yaml(source))
    };
    tree.map_err(|e| OpenApiError {
        document,
        path: String::new(),
        offset: 0,
        kind: OpenApiErrorKind::Syntax {
            route,
            message: e.to_string(),
        },
    })
}

const fn error(
    document: usize,
    path: String,
    offset: usize,
    kind: OpenApiErrorKind,
) -> OpenApiError {
    OpenApiError {
        document,
        path,
        offset,
        kind,
    }
}

fn child(pointer: &str, key: &str) -> String {
    format!("{pointer}/{}", pointer_token(key))
}

fn get<'n>(node: &'n Node, key: &str) -> Option<&'n Node> {
    node.mapping()
        .and_then(|entries| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v))
}

/// Whether `version` is an `OpenAPI` 3.1 version: `3.1.` followed by a
/// patch number, optionally with a `-` suffix (`^3\.1\.\d+(-.+)?$`).
fn version_3_1(version: &str) -> bool {
    version.strip_prefix("3.1.").is_some_and(|rest| {
        let (patch, suffix) = rest
            .split_once('-')
            .map_or((rest, None), |(p, s)| (p, Some(s)));
        !patch.is_empty()
            && patch.bytes().all(|b| b.is_ascii_digit())
            && suffix.is_none_or(|s| !s.is_empty())
    })
}

/// Whether `name` is a component name: `^[a-zA-Z0-9.\-_]+$`.
fn component_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Whether `key` is a key of the Responses Object's patterned fields:
/// `default`, a status code, or a range `1XX` to `5XX`.
fn status_key(key: &str) -> bool {
    let b = key.as_bytes();
    key == "default"
        || (b.len() == 3
            && (b'1'..=b'5').contains(&b[0])
            && ((b[1].is_ascii_digit() && b[2].is_ascii_digit()) || (b[1] == b'X' && b[2] == b'X')))
}

/// The template expressions of a path, in order, or `None` when a brace
/// is unpaired, nested or encloses nothing.
fn template_names(path: &str) -> Option<Vec<&str>> {
    let mut names = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find(['{', '}']) {
        if rest.as_bytes()[open] == b'}' {
            return None;
        }
        let after = &rest[open + 1..];
        let close = after.find(['{', '}'])?;
        if after.as_bytes()[close] == b'{' || close == 0 {
            return None;
        }
        names.push(&after[..close]);
        rest = &after[close + 1..];
    }
    Some(names)
}

/// The path with each template expression's name removed, so two paths
/// that differ only in their template names are equal.
fn template_shape(path: &str) -> String {
    let mut shape = String::with_capacity(path.len());
    let mut inside = false;
    for c in path.chars() {
        match c {
            '{' => {
                inside = true;
                shape.push('{');
            }
            '}' => {
                inside = false;
                shape.push('}');
            }
            _ if inside => {}
            _ => shape.push(c),
        }
    }
    shape
}

/// `fragment` with its `%XX` escapes decoded, or `None` when an escape is
/// malformed or the bytes are not UTF-8.
fn percent_decode(fragment: &str) -> Option<String> {
    let bytes = fragment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Check the value of an `OpenAPI` vocabulary keyword of a Schema Object:
/// `discriminator` an object with a string `propertyName` and a `mapping`
/// of strings, `xml` an object of strings and booleans, `externalDocs` an
/// External Documentation Object, `example` any value.
fn check_vocabulary<'n>(
    key: &str,
    value: &'n Node,
    at: &str,
) -> Result<(), (String, &'n Node, &'static str)> {
    let field = |name: &str| get(value, name);
    let fail = |path: String, node: &'n Node, rule| Err((path, node, rule));
    let is_extension = |k: &str| k.starts_with("x-");
    match key {
        "discriminator" => {
            let Some(entries) = value.mapping() else {
                return fail(at.to_owned(), value, "a discriminator is an object");
            };
            match field("propertyName") {
                Some(name) if name.string().is_some() => {}
                Some(name) => return fail(child(at, "propertyName"), name, "expected a string"),
                None => {
                    return fail(
                        at.to_owned(),
                        value,
                        "a discriminator requires propertyName",
                    );
                }
            }
            for (k, v) in entries {
                let ok = match k.as_str() {
                    "propertyName" => true,
                    "mapping" => v
                        .mapping()
                        .is_some_and(|m| m.iter().all(|(_, s)| s.string().is_some())),
                    other => is_extension(other),
                };
                if !ok {
                    return fail(
                        child(at, k),
                        v,
                        "not a field of a discriminator, or of the wrong type",
                    );
                }
            }
        }
        "xml" => {
            let Some(entries) = value.mapping() else {
                return fail(at.to_owned(), value, "xml is an object");
            };
            for (k, v) in entries {
                let ok = match k.as_str() {
                    "name" | "namespace" | "prefix" => v.string().is_some(),
                    "attribute" | "wrapped" => v.boolean().is_some(),
                    other => is_extension(other),
                };
                if !ok {
                    return fail(
                        child(at, k),
                        v,
                        "not a field of an XML Object, or of the wrong type",
                    );
                }
            }
        }
        "externalDocs" => {
            let Some(entries) = value.mapping() else {
                return fail(at.to_owned(), value, "externalDocs is an object");
            };
            if field("url").is_none() {
                return fail(at.to_owned(), value, "externalDocs requires url");
            }
            for (k, v) in entries {
                let ok = match k.as_str() {
                    "url" | "description" => v.string().is_some(),
                    other => is_extension(other),
                };
                if !ok {
                    return fail(
                        child(at, k),
                        v,
                        "not a field of an External Documentation Object, or of the wrong type",
                    );
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// The `OpenAPI` vocabulary of the OAS dialect.
const OAS_VOCABULARY: Vocabulary = Vocabulary {
    keywords: VOCABULARY,
    check: check_vocabulary,
};

/// The dialects a Schema Object may name: the OAS dialect and 2020-12.
const DIALECTS: &[Dialect] = &[
    Dialect {
        id: OAS_DIALECT,
        vocabulary: Some(OAS_VOCABULARY),
    },
    Dialect {
        id: DIALECT,
        vocabulary: None,
    },
];

/// A modelled object of the description: the kind its place takes (for a
/// Reference Object, the kind of what it stands for), whether it is a
/// Reference Object, and its node.
struct Slot<'n> {
    kind: &'static str,
    reference: bool,
    node: &'n Node,
}

/// A reference to resolve once every document is read.
struct Pending {
    document: usize,
    /// The pointer of the referring vertex.
    pointer: String,
    /// The reference text.
    text: String,
    /// The offset of the reference text.
    offset: usize,
    /// The kind the referenced object must have.
    expected: &'static str,
}

/// An edge from an object to a Schema Object, added once the Schema
/// Objects are built.
struct SchemaEdge {
    source: String,
    kind: &'static str,
    name: Option<String>,
    target: String,
}

/// A security requirement's scheme name, to resolve against the
/// components.
struct Requirement {
    document: usize,
    pointer: String,
    name: String,
    offset: usize,
}

struct Reader<'n> {
    trees: &'n [Node],
    kinds: Vec<DocumentKind>,
    /// Each document's retrieval URI, without an empty fragment.
    uris: Vec<Option<String>>,
    /// Each document's vertex-id prefix.
    prefixes: Vec<String>,
    /// The vocabulary each document's Schema Objects start with.
    vocabularies: Vec<Option<Vocabulary>>,
    document: usize,
    builder: Option<SchemaBuilder>,
    slots: HashMap<(usize, String), Slot<'n>>,
    pending: Vec<Pending>,
    schemas: Vec<Embedded<'n>>,
    schema_edges: Vec<SchemaEdge>,
    requirements: Vec<Requirement>,
    /// Each parameters list: its document, pointer and items' pointers.
    parameter_lists: Vec<(usize, String, Vec<String>)>,
    /// Every object read, in the order read: its kind, document and
    /// pointer.
    order: Vec<(&'static str, usize, String)>,
    /// Each referring place (a Reference Object, a Path Item with `$ref`)
    /// to the place it resolves to.
    targets: HashMap<(usize, String), (usize, String)>,
    roots: Vec<String>,
}

impl<'n> Reader<'n> {
    fn new(documents: &[ApiDocument<'_>], trees: &'n [Node]) -> R<Self> {
        let mut uris: Vec<Option<String>> = Vec::with_capacity(documents.len());
        for (i, (document, tree)) in documents.iter().zip(trees).enumerate() {
            let uri = match document.retrieval_uri {
                None => None,
                Some(uri) => {
                    let base = uri.strip_suffix('#').unwrap_or(uri);
                    if base.contains('#') || resolve_uri_against(None, base).is_none() {
                        return Err(error(
                            i,
                            String::new(),
                            tree.offset,
                            OpenApiErrorKind::Invalid(
                                "a retrieval URI must be absolute, with no fragment",
                            ),
                        ));
                    }
                    Some(base.to_owned())
                }
            };
            if uris.contains(&uri) {
                return Err(error(
                    i,
                    String::new(),
                    tree.offset,
                    OpenApiErrorKind::DuplicateDocument(uri.unwrap_or_default()),
                ));
            }
            uris.push(uri);
        }
        if documents
            .first()
            .is_some_and(|d| d.kind != DocumentKind::OpenApi)
        {
            return Err(error(
                0,
                String::new(),
                trees[0].offset,
                OpenApiErrorKind::Invalid("the entry document must be an OpenAPI document"),
            ));
        }
        let prefixes = uris.iter().map(|u| u.clone().unwrap_or_default()).collect();
        Ok(Self {
            trees,
            kinds: documents.iter().map(|d| d.kind).collect(),
            uris,
            prefixes,
            vocabularies: Vec::new(),
            document: 0,
            builder: Some(SchemaBuilder::new(&protocol::protocol())),
            slots: HashMap::new(),
            pending: Vec::new(),
            schemas: Vec::new(),
            schema_edges: Vec::new(),
            requirements: Vec::new(),
            parameter_lists: Vec::new(),
            order: Vec::new(),
            targets: HashMap::new(),
            roots: Vec::new(),
        })
    }

    const fn fail<T>(&self, path: String, node: &Node, kind: OpenApiErrorKind) -> R<T> {
        Err(error(self.document, path, node.offset, kind))
    }

    fn id(&self, pointer: &str) -> String {
        format!("{}#{pointer}", self.prefixes[self.document])
    }

    fn with(&mut self, step: impl FnOnce(SchemaBuilder) -> SchemaBuilder) {
        self.builder = self.builder.take().map(step);
    }

    fn try_with(
        &mut self,
        path: &str,
        offset: usize,
        step: impl FnOnce(SchemaBuilder) -> Result<SchemaBuilder, panproto_schema::SchemaError>,
    ) -> R<()> {
        let Some(builder) = self.builder.take() else {
            return Ok(());
        };
        let built = step(builder).map_err(|e| {
            error(
                self.document,
                path.to_owned(),
                offset,
                OpenApiErrorKind::Build(e.to_string()),
            )
        })?;
        self.builder = Some(built);
        Ok(())
    }

    fn json(&self, node: &Node, at: &str) -> R<String> {
        node.json_text().map_err(|e| {
            error(
                self.document,
                at.to_owned(),
                node.offset,
                OpenApiErrorKind::Unsupported(e.feature()),
            )
        })
    }

    /// Read the document `self.document`: its version and its objects.
    fn read_document(&mut self) -> R<()> {
        let tree: &'n Node = &self.trees[self.document];
        if self.kinds[self.document] == DocumentKind::Schema {
            self.vocabularies.push(Some(OAS_VOCABULARY));
            self.schemas.push(Embedded {
                document: self.document,
                pointer: String::new(),
                node: tree,
            });
            let id = self.id("");
            self.roots.push(id);
            return Ok(());
        }
        if tree.mapping().is_none() {
            return self.fail(
                String::new(),
                tree,
                OpenApiErrorKind::WrongType("an object"),
            );
        }
        if let Some(swagger) = get(tree, "swagger") {
            let text = swagger.json_text().unwrap_or_default();
            let text = swagger.string().map_or(text, str::to_owned);
            return self.fail("/swagger".into(), swagger, OpenApiErrorKind::Swagger(text));
        }
        let Some(version) = get(tree, "openapi") else {
            return self.fail(String::new(), tree, OpenApiErrorKind::NotOpenApi);
        };
        match version.string() {
            None => {
                return self.fail(
                    "/openapi".into(),
                    version,
                    OpenApiErrorKind::WrongType("a string"),
                );
            }
            Some(text) if !version_3_1(text) => {
                return self.fail(
                    "/openapi".into(),
                    version,
                    OpenApiErrorKind::Version(text.to_owned()),
                );
            }
            Some(_) => {}
        }
        let vocabulary = match get(tree, "jsonSchemaDialect") {
            None => Some(OAS_VOCABULARY),
            Some(dialect) => {
                let text = dialect.string().unwrap_or_default();
                let named = DIALECTS
                    .iter()
                    .find(|d| d.id == text.strip_suffix('#').unwrap_or(text));
                match named {
                    Some(d) => d.vocabulary,
                    None if dialect.string().is_none() => {
                        return self.fail(
                            "/jsonSchemaDialect".into(),
                            dialect,
                            OpenApiErrorKind::WrongType("a string"),
                        );
                    }
                    None => {
                        return self.fail(
                            "/jsonSchemaDialect".into(),
                            dialect,
                            OpenApiErrorKind::Dialect(text.to_owned()),
                        );
                    }
                }
            }
        };
        self.vocabularies.push(vocabulary);
        let root = self.object("openapi", tree, "")?;
        self.with(|b| b.entry(&root));
        self.roots.push(root);
        Ok(())
    }

    /// Read the object of kind `kind` at `pointer` (not a Reference Object)
    /// into its vertex, returning the vertex id.
    fn object(&mut self, kind: &'static str, node: &'n Node, pointer: &str) -> R<String> {
        let Some(entries) = node.mapping() else {
            return self.fail(
                pointer.to_owned(),
                node,
                OpenApiErrorKind::WrongType("an object"),
            );
        };
        let Some(spec) = protocol::object(kind) else {
            return self.fail(
                pointer.to_owned(),
                node,
                OpenApiErrorKind::Build(format!("no object kind {kind}")),
            );
        };
        let patterned = spec
            .fields
            .iter()
            .find(|f| matches!(f.holds, Holds::Patterned { .. }));
        for (key, value) in entries {
            let known = key.starts_with("x-")
                || spec.fields.iter().any(|f| f.name == key)
                || (kind == "path-item" && key == "$ref")
                || patterned.is_some()
                || kind == "security-requirement";
            if !known {
                return self.fail(
                    child(pointer, key),
                    value,
                    OpenApiErrorKind::UnknownField(key.clone()),
                );
            }
        }
        for field in spec.fields {
            if field.required && !field.name.is_empty() && get(node, field.name).is_none() {
                return self.fail(
                    pointer.to_owned(),
                    node,
                    OpenApiErrorKind::Missing(field.name),
                );
            }
        }
        self.check_object(kind, node, pointer)?;
        let id = self.id(pointer);
        self.try_with(pointer, node.offset, |b| b.vertex(&id, kind, None))?;
        self.slots.insert(
            (self.document, pointer.to_owned()),
            Slot {
                kind,
                reference: false,
                node,
            },
        );
        self.order.push((kind, self.document, pointer.to_owned()));
        if kind == "path-item"
            && let Some(reference) = get(node, "$ref")
        {
            self.reference_text(reference, pointer, "path-item")?;
        }
        for field in spec.fields {
            if let Holds::Patterned { .. } = field.holds {
                self.patterned(kind, &id, node, pointer, field)?;
            } else if let Some(value) = get(node, field.name) {
                self.field(&id, field, value, &child(pointer, field.name))?;
            }
        }
        if kind == "security-requirement" {
            self.requirement(&id, node, pointer)?;
        }
        for (key, value) in entries {
            if key.starts_with("x-") && kind != "security-requirement" {
                let text = self.json(value, &child(pointer, key))?;
                let member = format!("{{{}:{text}}}", json_string(key));
                self.with(|b| b.constraint(&id, EXTENSION, &member));
            }
        }
        Ok(id)
    }

    /// Record the `$ref` of a Reference Object or Path Item at `pointer`.
    fn reference_text(&mut self, reference: &Node, pointer: &str, expected: &'static str) -> R<()> {
        let Some(text) = reference.string() else {
            return self.fail(
                child(pointer, "$ref"),
                reference,
                OpenApiErrorKind::WrongType("a string"),
            );
        };
        self.pending.push(Pending {
            document: self.document,
            pointer: pointer.to_owned(),
            text: text.to_owned(),
            offset: reference.offset,
            expected,
        });
        Ok(())
    }

    /// Read the object at `pointer` that may be a Reference Object to an
    /// object of kind `kind`.
    fn object_or_reference(
        &mut self,
        kind: &'static str,
        node: &'n Node,
        pointer: &str,
    ) -> R<String> {
        let Some(reference) = get(node, "$ref") else {
            return self.object(kind, node, pointer);
        };
        let id = self.id(pointer);
        self.reference_text(reference, pointer, kind)?;
        self.try_with(pointer, node.offset, |b| b.vertex(&id, REFERENCE, None))?;
        self.slots.insert(
            (self.document, pointer.to_owned()),
            Slot {
                kind,
                reference: true,
                node,
            },
        );
        for field in REFERENCE_FIELDS {
            if let Some(value) = get(node, field.name) {
                self.field(&id, field, value, &child(pointer, field.name))?;
            }
        }
        for (key, value) in node.mapping().unwrap_or_default() {
            if key != "$ref" && !REFERENCE_FIELDS.iter().any(|f| f.name == key) {
                let text = self.json(value, &child(pointer, key))?;
                let member = format!("{{{}:{text}}}", json_string(key));
                self.with(|b| b.constraint(&id, IGNORED, &member));
            }
        }
        Ok(id)
    }

    /// Read one fixed field of the object `id`.
    fn field(&mut self, id: &str, field: &Field, value: &'n Node, at: &str) -> R<()> {
        match field.holds {
            Holds::Str
            | Holds::Bool
            | Holds::Any
            | Holds::Strings
            | Holds::StringMap
            | Holds::AnyMap => self.value_field(id, field, value, at),
            Holds::One(kind) => {
                let target = self.object(kind, value, at)?;
                self.try_with(at, value.offset, |b| b.edge(id, &target, field.edge, None))
            }
            Holds::OneOrRef(kind) => {
                let target = self.object_or_reference(kind, value, at)?;
                self.try_with(at, value.offset, |b| b.edge(id, &target, field.edge, None))
            }
            Holds::Schema => {
                self.schema(id, field.edge, None, value, at);
                Ok(())
            }
            Holds::List(_) | Holds::ListOrRef(_) => self.list_field(id, field, value, at),
            Holds::Map(_) | Holds::MapOrRef(_) | Holds::SchemaMap => {
                self.map_field(id, field, value, at)
            }
            Holds::Patterned { .. } => Ok(()),
        }
    }

    /// Read a field holding a value into a constraint of its name.
    fn value_field(&mut self, id: &str, field: &Field, value: &Node, at: &str) -> R<()> {
        let ok = match field.holds {
            Holds::Str => value.string().is_some(),
            Holds::Bool => value.boolean().is_some(),
            Holds::Strings => value
                .sequence()
                .is_some_and(|items| items.iter().all(|i| i.string().is_some())),
            Holds::StringMap => value
                .mapping()
                .is_some_and(|entries| entries.iter().all(|(_, v)| v.string().is_some())),
            Holds::AnyMap => value.mapping().is_some(),
            _ => true,
        };
        if !ok {
            let expected = match field.holds {
                Holds::Str => "a string",
                Holds::Bool => "a boolean",
                Holds::Strings => "an array of strings",
                Holds::StringMap => "an object of strings",
                _ => "an object",
            };
            return self.fail(at.to_owned(), value, OpenApiErrorKind::WrongType(expected));
        }
        let text = self.json(value, at)?;
        self.with(|b| b.constraint(id, field.name, &text));
        Ok(())
    }

    /// Read a list field: an edge per item, named by its index, or the
    /// constraint `[]` for an empty list.
    fn list_field(&mut self, id: &str, field: &Field, value: &'n Node, at: &str) -> R<()> {
        let Some(items) = value.sequence() else {
            return self.fail(
                at.to_owned(),
                value,
                OpenApiErrorKind::WrongType("an array"),
            );
        };
        if items.is_empty() {
            self.with(|b| b.constraint(id, field.name, "[]"));
        }
        let mut pointers = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            let item_at = format!("{at}/{i}");
            let target = match field.holds {
                Holds::ListOrRef(kind) => self.object_or_reference(kind, item, &item_at)?,
                Holds::List(kind) => self.object(kind, item, &item_at)?,
                _ => continue,
            };
            let name = i.to_string();
            self.try_with(&item_at, item.offset, |b| {
                b.edge(id, &target, field.edge, Some(&name))
            })?;
            pointers.push(item_at);
        }
        if field.name == "parameters" {
            self.parameter_lists
                .push((self.document, at.to_owned(), pointers));
        }
        Ok(())
    }

    /// Read a map field: an edge per entry, named by its key, or the
    /// constraint `{}` for an empty map.
    fn map_field(&mut self, id: &str, field: &Field, value: &'n Node, at: &str) -> R<()> {
        let Some(entries) = value.mapping() else {
            return self.fail(
                at.to_owned(),
                value,
                OpenApiErrorKind::WrongType("an object"),
            );
        };
        if entries.is_empty() {
            self.with(|b| b.constraint(id, field.name, "{}"));
        }
        let components = at.starts_with("/components/") && at.matches('/').count() == 2;
        for (key, entry) in entries {
            let entry_at = child(at, key);
            if components && !component_name(key) {
                return self.fail(
                    entry_at,
                    entry,
                    OpenApiErrorKind::ComponentName(key.clone()),
                );
            }
            let target = match field.holds {
                Holds::Map(kind) => self.object(kind, entry, &entry_at)?,
                Holds::MapOrRef(kind) => self.object_or_reference(kind, entry, &entry_at)?,
                _ => {
                    self.schema(id, field.edge, Some(key.clone()), entry, &entry_at);
                    continue;
                }
            };
            self.try_with(&entry_at, entry.offset, |b| {
                b.edge(id, &target, field.edge, Some(key))
            })?;
        }
        Ok(())
    }

    /// Record the Schema Object at `at` and the edge to it from `id`.
    fn schema(
        &mut self,
        id: &str,
        edge: &'static str,
        name: Option<String>,
        node: &'n Node,
        at: &str,
    ) {
        self.schemas.push(Embedded {
            document: self.document,
            pointer: at.to_owned(),
            node,
        });
        self.schema_edges.push(SchemaEdge {
            source: id.to_owned(),
            kind: edge,
            name,
            target: self.id(at),
        });
    }

    /// Read the patterned fields of a Paths, Responses or Callback object.
    fn patterned(
        &mut self,
        kind: &'static str,
        id: &str,
        node: &'n Node,
        pointer: &str,
        field: &Field,
    ) -> R<()> {
        let Holds::Patterned {
            kind: target,
            or_ref,
        } = field.holds
        else {
            return Ok(());
        };
        let edge = field.edge;
        let mut any = false;
        for (key, value) in node.mapping().unwrap_or_default() {
            if key.starts_with("x-") {
                continue;
            }
            let at = child(pointer, key);
            match kind {
                "paths" if !key.starts_with('/') => {
                    return self.fail(
                        at,
                        value,
                        OpenApiErrorKind::Invalid("a path must begin with /"),
                    );
                }
                "responses" if !status_key(key) => {
                    return self.fail(
                        at,
                        value,
                        OpenApiErrorKind::Invalid(
                            "a response is keyed by default, a status code or 1XX to 5XX",
                        ),
                    );
                }
                _ => {}
            }
            any = true;
            let vertex = if or_ref {
                self.object_or_reference(target, value, &at)?
            } else {
                self.object(target, value, &at)?
            };
            self.try_with(&at, value.offset, |b| b.edge(id, &vertex, edge, Some(key)))?;
        }
        if kind == "responses" && !any {
            return self.fail(
                pointer.to_owned(),
                node,
                OpenApiErrorKind::Invalid(
                    "the Responses Object must contain at least one response code",
                ),
            );
        }
        Ok(())
    }

    /// Read the entries of a Security Requirement: a [`SCOPES`] constraint
    /// each, resolved to its scheme later.
    fn requirement(&mut self, id: &str, node: &Node, pointer: &str) -> R<()> {
        for (name, scopes) in node.mapping().unwrap_or_default() {
            let at = child(pointer, name);
            if !scopes
                .sequence()
                .is_some_and(|items| items.iter().all(|i| i.string().is_some()))
            {
                return self.fail(
                    at,
                    scopes,
                    OpenApiErrorKind::WrongType("an array of strings"),
                );
            }
            let text = self.json(scopes, &at)?;
            let member = format!("{{{}:{text}}}", json_string(name));
            self.with(|b| b.constraint(id, SCOPES, &member));
            self.requirements.push(Requirement {
                document: self.document,
                pointer: pointer.to_owned(),
                name: name.clone(),
                offset: scopes.offset,
            });
        }
        Ok(())
    }
}

/// The `style` values each parameter location allows.
fn styles(location: &str) -> &'static [&'static str] {
    match location {
        "path" => &["matrix", "label", "simple"],
        "query" => &["form", "spaceDelimited", "pipeDelimited", "deepObject"],
        "header" => &["simple"],
        "cookie" => &["form"],
        _ => &[],
    }
}

/// The fields a Parameter or Header Object uses only with `schema`.
const WITH_SCHEMA: &[&str] = &["style", "explode", "allowReserved", "example", "examples"];

impl<'n> Reader<'n> {
    /// The rules of one object that its field table does not state.
    fn check_object(&self, kind: &str, node: &Node, pointer: &str) -> R<()> {
        match kind {
            "openapi" => self.check_root(node, pointer),
            "license" => self.exclusive(node, pointer, "identifier", "url"),
            "server" => match get(node, "url") {
                Some(url) if url.string().is_some_and(|u| u.contains(['?', '#'])) => self.fail(
                    child(pointer, "url"),
                    url,
                    OpenApiErrorKind::Invalid("a server URL has no query or fragment"),
                ),
                _ => Ok(()),
            },
            "server-variable" => self.check_variable(node, pointer),
            "parameter" => self.check_parameter(node, pointer),
            "header" => self.check_header(node, pointer),
            "media-type" => self.exclusive(node, pointer, "example", "examples"),
            "encoding" => match get(node, "style") {
                Some(style) if style.string().is_some_and(|s| !styles("query").contains(&s)) => {
                    self.fail(
                        child(pointer, "style"),
                        style,
                        OpenApiErrorKind::Invalid(
                            "an encoding's style is form, spaceDelimited, pipeDelimited or deepObject",
                        ),
                    )
                }
                _ => Ok(()),
            },
            "example" => self.exclusive(node, pointer, "value", "externalValue"),
            "link" => {
                self.exclusive(node, pointer, "operationRef", "operationId")?;
                if get(node, "operationRef").is_none() && get(node, "operationId").is_none() {
                    return self.fail(
                        pointer.to_owned(),
                        node,
                        OpenApiErrorKind::Missing("operationRef or operationId"),
                    );
                }
                Ok(())
            }
            "security-scheme" => self.check_scheme(node, pointer),
            "oauth-flows" => self.check_flows(node, pointer),
            _ => Ok(()),
        }
    }

    fn exclusive(&self, node: &Node, pointer: &str, a: &'static str, b: &'static str) -> R<()> {
        if let (Some(_), Some(second)) = (get(node, a), get(node, b)) {
            return self.fail(child(pointer, b), second, OpenApiErrorKind::Exclusive(a, b));
        }
        Ok(())
    }

    fn check_root(&self, node: &Node, pointer: &str) -> R<()> {
        if ["paths", "components", "webhooks"]
            .iter()
            .all(|k| get(node, k).is_none())
        {
            return self.fail(
                pointer.to_owned(),
                node,
                OpenApiErrorKind::Missing("paths, components or webhooks"),
            );
        }
        let mut seen = HashSet::new();
        let tags = get(node, "tags")
            .and_then(Node::sequence)
            .unwrap_or_default();
        for (i, tag) in tags.iter().enumerate() {
            if let Some(name) = get(tag, "name").and_then(Node::string)
                && !seen.insert(name)
            {
                return self.fail(
                    format!("{pointer}/tags/{i}/name"),
                    tag,
                    OpenApiErrorKind::DuplicateTag(name.to_owned()),
                );
            }
        }
        Ok(())
    }

    fn check_variable(&self, node: &Node, pointer: &str) -> R<()> {
        let Some(values) = get(node, "enum") else {
            return Ok(());
        };
        let Some(items) = values.sequence() else {
            return Ok(());
        };
        if items.is_empty() {
            return self.fail(
                child(pointer, "enum"),
                values,
                OpenApiErrorKind::Invalid("a server variable's enum must not be empty"),
            );
        }
        if let Some(default) = get(node, "default")
            && let Some(text) = default.string()
            && !items.iter().any(|i| i.string() == Some(text))
        {
            return self.fail(
                child(pointer, "default"),
                default,
                OpenApiErrorKind::Invalid("a server variable's default must be one of its enum"),
            );
        }
        Ok(())
    }

    /// The rules a Parameter and a Header share: `schema` or `content` but
    /// not both, one media type in `content`, `example` or `examples`, and
    /// no field of the `schema` strategy beside `content`.
    fn check_strategy(&self, node: &Node, pointer: &str) -> R<()> {
        match (get(node, "schema"), get(node, "content")) {
            (Some(_), Some(content)) => {
                return self.fail(
                    child(pointer, "content"),
                    content,
                    OpenApiErrorKind::Exclusive("schema", "content"),
                );
            }
            (None, None) => {
                return self.fail(
                    pointer.to_owned(),
                    node,
                    OpenApiErrorKind::Missing("schema or content"),
                );
            }
            (None, Some(content)) => {
                if content.mapping().is_some_and(|m| m.len() != 1) {
                    return self.fail(
                        child(pointer, "content"),
                        content,
                        OpenApiErrorKind::Invalid("content must hold exactly one media type"),
                    );
                }
                for name in WITH_SCHEMA {
                    if let Some(value) = get(node, name) {
                        return self.fail(
                            child(pointer, name),
                            value,
                            OpenApiErrorKind::Invalid("this field is used only with schema"),
                        );
                    }
                }
            }
            (Some(_), None) => {}
        }
        self.exclusive(node, pointer, "example", "examples")
    }

    fn check_parameter(&self, node: &Node, pointer: &str) -> R<()> {
        let location = get(node, "in").and_then(Node::string);
        if let Some(text) = location
            && styles(text).is_empty()
        {
            return self.fail(
                child(pointer, "in"),
                get(node, "in").unwrap_or(node),
                OpenApiErrorKind::Invalid("in is query, header, path or cookie"),
            );
        }
        self.check_strategy(node, pointer)?;
        if location.is_some_and(|l| l != "query") {
            for (name, rule) in [
                (
                    "allowEmptyValue",
                    "allowEmptyValue applies only to query parameters",
                ),
                (
                    "allowReserved",
                    "allowReserved applies only to query parameters",
                ),
            ] {
                if let Some(value) = get(node, name) {
                    return self.fail(child(pointer, name), value, OpenApiErrorKind::Invalid(rule));
                }
            }
        }
        if let (Some(text), Some(style)) = (location, get(node, "style"))
            && let Some(name) = style.string()
            && !styles(text).contains(&name)
        {
            return self.fail(
                child(pointer, "style"),
                style,
                OpenApiErrorKind::Invalid("the style does not apply to the parameter's location"),
            );
        }
        if location == Some("path") && get(node, "required").and_then(Node::boolean) != Some(true) {
            return self.fail(
                pointer.to_owned(),
                node,
                OpenApiErrorKind::Invalid("a path parameter must have required: true"),
            );
        }
        Ok(())
    }

    fn check_header(&self, node: &Node, pointer: &str) -> R<()> {
        self.check_strategy(node, pointer)?;
        if let Some(style) = get(node, "style")
            && style.string().is_some_and(|s| s != "simple")
        {
            return self.fail(
                child(pointer, "style"),
                style,
                OpenApiErrorKind::Invalid("a header's style is simple"),
            );
        }
        Ok(())
    }

    fn check_scheme(&self, node: &Node, pointer: &str) -> R<()> {
        let Some(kind) = get(node, "type").and_then(Node::string) else {
            return Ok(());
        };
        let (allowed, required): (&[&str], &[&'static str]) = match kind {
            "apiKey" => (&["name", "in"], &["name", "in"]),
            "http" => (&["scheme", "bearerFormat"], &["scheme"]),
            "mutualTLS" => (&[], &[]),
            "oauth2" => (&["flows"], &["flows"]),
            "openIdConnect" => (&["openIdConnectUrl"], &["openIdConnectUrl"]),
            _ => {
                return self.fail(
                    child(pointer, "type"),
                    get(node, "type").unwrap_or(node),
                    OpenApiErrorKind::Invalid(
                        "type is apiKey, http, mutualTLS, oauth2 or openIdConnect",
                    ),
                );
            }
        };
        for name in [
            "name",
            "in",
            "scheme",
            "bearerFormat",
            "flows",
            "openIdConnectUrl",
        ] {
            if let Some(value) = get(node, name)
                && !allowed.contains(&name)
            {
                return self.fail(
                    child(pointer, name),
                    value,
                    OpenApiErrorKind::Invalid("this field does not apply to the scheme's type"),
                );
            }
        }
        for name in required {
            if get(node, name).is_none() {
                return self.fail(pointer.to_owned(), node, OpenApiErrorKind::Missing(name));
            }
        }
        if let Some(location) = get(node, "in")
            && kind == "apiKey"
            && location
                .string()
                .is_some_and(|l| !matches!(l, "query" | "header" | "cookie"))
        {
            return self.fail(
                child(pointer, "in"),
                location,
                OpenApiErrorKind::Invalid("an API key is in query, header or cookie"),
            );
        }
        if let Some(format) = get(node, "bearerFormat")
            && !get(node, "scheme")
                .and_then(Node::string)
                .is_some_and(|s| s.eq_ignore_ascii_case("bearer"))
        {
            return self.fail(
                child(pointer, "bearerFormat"),
                format,
                OpenApiErrorKind::Invalid("bearerFormat applies only to the bearer scheme"),
            );
        }
        Ok(())
    }

    fn check_flows(&self, node: &Node, pointer: &str) -> R<()> {
        let rules: [(&str, &[&'static str], &str); 4] = [
            ("implicit", &["authorizationUrl"], "tokenUrl"),
            ("password", &["tokenUrl"], "authorizationUrl"),
            ("clientCredentials", &["tokenUrl"], "authorizationUrl"),
            ("authorizationCode", &["authorizationUrl", "tokenUrl"], ""),
        ];
        for (flow, required, forbidden) in rules {
            let Some(value) = get(node, flow) else {
                continue;
            };
            if value.mapping().is_none() {
                continue;
            }
            let at = child(pointer, flow);
            for name in required {
                if get(value, name).is_none() {
                    return self.fail(at, value, OpenApiErrorKind::Missing(name));
                }
            }
            if let Some(extra) = get(value, forbidden) {
                return self.fail(
                    child(&at, forbidden),
                    extra,
                    OpenApiErrorKind::Invalid("this URL does not apply to the flow"),
                );
            }
        }
        Ok(())
    }

    /// The document and JSON pointer a reference made in `document` names.
    fn locate(&self, document: usize, text: &str) -> Result<(usize, String), OpenApiErrorKind> {
        let (target, fragment) = if let Some(fragment) = text.strip_prefix('#') {
            (document, fragment.to_owned())
        } else {
            let resolved = self.uris[document]
                .as_ref()
                .map_or_else(
                    || resolve_uri_against(None, text),
                    |base| resolve_uri_reference(base, text),
                )
                .ok_or_else(|| OpenApiErrorKind::External(text.to_owned()))?;
            let (uri, fragment) = resolved.split_once('#').unwrap_or((&resolved, ""));
            let target = self
                .uris
                .iter()
                .position(|u| u.as_deref() == Some(uri))
                .ok_or_else(|| OpenApiErrorKind::External(text.to_owned()))?;
            (target, fragment.to_owned())
        };
        if !fragment.is_empty() && !fragment.starts_with('/') {
            return Err(OpenApiErrorKind::RefTarget(text.to_owned()));
        }
        let pointer = percent_decode(&fragment)
            .ok_or_else(|| OpenApiErrorKind::RefTarget(text.to_owned()))?;
        Ok((target, pointer))
    }

    /// Resolve each Reference Object and Path Item `$ref` to its target,
    /// adding a [`REF`] edge, and refuse a cycle of references.
    fn resolve_references(&mut self) -> R<()> {
        let pending = std::mem::take(&mut self.pending);
        for reference in &pending {
            let at = child(&reference.pointer, "$ref");
            let fail = |kind| {
                Err(error(
                    reference.document,
                    at.clone(),
                    reference.offset,
                    kind,
                ))
            };
            let (document, pointer) = match self.locate(reference.document, &reference.text) {
                Ok(place) => place,
                Err(kind) => return fail(kind),
            };
            let Some(slot) = self.slots.get(&(document, pointer.clone())) else {
                return fail(OpenApiErrorKind::RefTarget(reference.text.clone()));
            };
            if slot.kind != reference.expected {
                return fail(OpenApiErrorKind::RefKind {
                    reference: reference.text.clone(),
                    expected: reference.expected,
                    found: slot.kind,
                });
            }
            let source = format!(
                "{}#{}",
                self.prefixes[reference.document], reference.pointer
            );
            let target = format!("{}#{pointer}", self.prefixes[document]);
            self.document = reference.document;
            self.try_with(&at, reference.offset, |b| {
                b.edge(&source, &target, REF, Some(&reference.text))
            })?;
            self.targets.insert(
                (reference.document, reference.pointer.clone()),
                (document, pointer),
            );
        }
        for reference in &pending {
            let start = (reference.document, reference.pointer.clone());
            let mut seen = HashSet::from([start.clone()]);
            let mut place = start;
            while let Some(next) = self.targets.get(&place) {
                if !seen.insert(next.clone()) {
                    return Err(error(
                        reference.document,
                        child(&reference.pointer, "$ref"),
                        reference.offset,
                        OpenApiErrorKind::RefCycle(reference.text.clone()),
                    ));
                }
                place = next.clone();
            }
        }
        Ok(())
    }

    /// The object a place finally stands for, through any references.
    fn resolved(&self, place: (usize, String)) -> Option<&'n Node> {
        let mut place = place;
        loop {
            let slot = self.slots.get(&place)?;
            if !slot.reference {
                return Some(slot.node);
            }
            place = self.targets.get(&place)?.clone();
        }
    }

    /// Build the Schema Objects with the JSON Schema reader and the edges
    /// to them.
    fn read_schemas(&mut self) -> R<()> {
        let host = Host {
            documents: self
                .uris
                .iter()
                .map(Option::as_deref)
                .zip(self.vocabularies.iter().copied())
                .collect(),
            schemas: std::mem::take(&mut self.schemas),
            dialects: DIALECTS,
        };
        let Some(builder) = self.builder.take() else {
            return Ok(());
        };
        let (builder, _) = build_embedded(&host, builder).map_err(|e| {
            error(
                e.document,
                e.path,
                e.offset,
                OpenApiErrorKind::Schema(e.kind),
            )
        })?;
        self.builder = Some(builder);
        for edge in std::mem::take(&mut self.schema_edges) {
            self.try_with("", 0, |b| {
                b.edge(&edge.source, &edge.target, edge.kind, edge.name.as_deref())
            })?;
        }
        Ok(())
    }

    /// The objects of kind `kind`, in the order read.
    fn read_of(&self, kind: &str) -> Vec<(usize, String, &'n Node)> {
        self.order
            .iter()
            .filter(|(k, _, _)| *k == kind)
            .filter_map(|(_, d, p)| {
                self.slots
                    .get(&(*d, p.clone()))
                    .map(|s| (*d, p.clone(), s.node))
            })
            .collect()
    }

    /// Each operation id, unique among the description's operations, to
    /// its operation.
    fn operation_ids(&self) -> R<HashMap<String, (usize, String)>> {
        let mut ids: HashMap<String, (usize, String)> = HashMap::new();
        for (document, pointer, node) in self.read_of("operation") {
            if let Some(value) = get(node, "operationId")
                && let Some(id) = value.string()
            {
                if ids.contains_key(id) {
                    return Err(error(
                        document,
                        child(&pointer, "operationId"),
                        value.offset,
                        OpenApiErrorKind::DuplicateOperationId(id.to_owned()),
                    ));
                }
                ids.insert(id.to_owned(), (document, pointer));
            }
        }
        Ok(ids)
    }

    fn check_operations(&self) -> R<()> {
        self.operation_ids().map(|_| ())
    }

    /// Resolve each Link's `operationId` or `operationRef` to an
    /// [`protocol::OPERATION`] edge.
    fn check_links(&mut self) -> R<()> {
        let ids = self.operation_ids()?;
        for (document, pointer, node) in self.read_of("link") {
            let (field, target) = if let Some(value) = get(node, "operationId") {
                let text = value.string().unwrap_or_default();
                let Some(target) = ids.get(text) else {
                    return Err(error(
                        document,
                        child(&pointer, "operationId"),
                        value.offset,
                        OpenApiErrorKind::UnknownOperationId(text.to_owned()),
                    ));
                };
                (value, target.clone())
            } else if let Some(value) = get(node, "operationRef") {
                let text = value.string().unwrap_or_default();
                let at = child(&pointer, "operationRef");
                let fail = |kind| Err(error(document, at.clone(), value.offset, kind));
                let place = match self.locate(document, text) {
                    Ok(place) => place,
                    Err(kind) => return fail(kind),
                };
                match self.slots.get(&place) {
                    Some(slot) if slot.kind == "operation" => {}
                    Some(slot) => {
                        return fail(OpenApiErrorKind::RefKind {
                            reference: text.to_owned(),
                            expected: "operation",
                            found: slot.kind,
                        });
                    }
                    None => return fail(OpenApiErrorKind::RefTarget(text.to_owned())),
                }
                (value, place)
            } else {
                continue;
            };
            let source = format!("{}#{pointer}", self.prefixes[document]);
            let target = format!("{}#{}", self.prefixes[target.0], target.1);
            self.document = document;
            self.try_with(&pointer, field.offset, |b| {
                b.edge(&source, &target, protocol::OPERATION, None)
            })?;
        }
        Ok(())
    }

    /// Resolve each Security Requirement's scheme names to the security
    /// schemes of its document's components.
    fn check_requirements(&mut self) -> R<()> {
        for requirement in std::mem::take(&mut self.requirements) {
            let target = child("/components/securitySchemes", &requirement.name);
            let at = child(&requirement.pointer, &requirement.name);
            match self.slots.get(&(requirement.document, target.clone())) {
                Some(slot) if slot.kind == "security-scheme" => {}
                _ => {
                    return Err(error(
                        requirement.document,
                        at,
                        requirement.offset,
                        OpenApiErrorKind::UnknownScheme(requirement.name),
                    ));
                }
            }
            let prefix = &self.prefixes[requirement.document];
            let source = format!("{prefix}#{}", requirement.pointer);
            let target = format!("{prefix}#{target}");
            self.document = requirement.document;
            self.try_with(&at, requirement.offset, |b| {
                b.edge(&source, &target, REQUIREMENT, Some(&requirement.name))
            })?;
        }
        Ok(())
    }

    /// The name and location of the parameter at a place.
    fn parameter(&self, document: usize, pointer: &str) -> Option<(&'n str, &'n str)> {
        let node = self.resolved((document, pointer.to_owned()))?;
        Some((
            get(node, "name").and_then(Node::string)?,
            get(node, "in").and_then(Node::string)?,
        ))
    }

    /// No parameters list holds two parameters of one name and location.
    fn check_parameter_lists(&self) -> R<()> {
        for (document, _, items) in &self.parameter_lists {
            let mut seen = HashSet::new();
            for item in items {
                if let Some((name, location)) = self.parameter(*document, item)
                    && !seen.insert((name, location))
                {
                    let offset = self
                        .slots
                        .get(&(*document, item.clone()))
                        .map_or(0, |s| s.node.offset);
                    return Err(error(
                        *document,
                        item.clone(),
                        offset,
                        OpenApiErrorKind::DuplicateParameter {
                            name: name.to_owned(),
                            location: location.to_owned(),
                        },
                    ));
                }
            }
        }
        Ok(())
    }

    /// The path parameters of a parameters list at `list`: each one's
    /// name and place.
    fn path_parameters(&self, document: usize, list: &str) -> Vec<(&'n str, String)> {
        self.parameter_lists
            .iter()
            .filter(|(d, p, _)| *d == document && p == list)
            .flat_map(|(_, _, items)| items)
            .filter_map(|item| {
                let (name, location) = self.parameter(document, item)?;
                (location == "path").then(|| (name, item.clone()))
            })
            .collect()
    }

    /// The rules of the paths: well-formed templates, no two paths equal
    /// but for template names, each template expression a path parameter
    /// of every operation, and each path parameter a template expression.
    fn check_paths(&self) -> R<()> {
        for (document, tree) in self.trees.iter().enumerate() {
            let Some(paths) = get(tree, "paths").and_then(Node::mapping) else {
                continue;
            };
            if self.kinds[document] != DocumentKind::OpenApi {
                continue;
            }
            let mut shapes = HashSet::new();
            for (path, item) in paths.iter().filter(|(k, _)| k.starts_with('/')) {
                let at = child("/paths", path);
                let fail = |kind| Err(error(document, at.clone(), item.offset, kind));
                let Some(names) = template_names(path) else {
                    return fail(OpenApiErrorKind::Template(path.clone()));
                };
                if !shapes.insert(template_shape(path)) {
                    return fail(OpenApiErrorKind::DuplicatePath(path.clone()));
                }
                self.check_path_item(document, &at, &names)?;
            }
        }
        Ok(())
    }

    fn check_path_item(&self, document: usize, at: &str, names: &[&str]) -> R<()> {
        let mut place = (document, at.to_owned());
        let mut chain = vec![place.clone()];
        while let Some(next) = self.targets.get(&place) {
            chain.push(next.clone());
            place = next.clone();
        }
        let mut shared = Vec::new();
        let mut operations: Vec<(usize, String)> = Vec::new();
        let mut methods = HashSet::new();
        for (d, item) in &chain {
            shared.extend(
                self.path_parameters(*d, &child(item, "parameters"))
                    .into_iter()
                    .map(|(n, p)| (*d, n, p)),
            );
            for method in METHODS {
                let op = child(item, method);
                if self.slots.contains_key(&(*d, op.clone())) && methods.insert(*method) {
                    operations.push((*d, op));
                }
            }
        }
        let unknown = |d: usize, name: &str, place: &str| {
            let offset = self
                .slots
                .get(&(d, place.to_owned()))
                .map_or(0, |s| s.node.offset);
            Err(error(
                d,
                place.to_owned(),
                offset,
                OpenApiErrorKind::UnknownPathParameter(name.to_owned()),
            ))
        };
        for (d, name, place) in &shared {
            if !names.contains(name) {
                return unknown(*d, name, place);
            }
        }
        for (d, op) in &operations {
            let own = self.path_parameters(*d, &child(op, "parameters"));
            for (name, place) in &own {
                if !names.contains(name) {
                    return unknown(*d, name, place);
                }
            }
            for name in names {
                if !own.iter().any(|(n, _)| n == name) && !shared.iter().any(|(_, n, _)| n == name)
                {
                    let offset = self
                        .slots
                        .get(&(*d, op.clone()))
                        .map_or(0, |s| s.node.offset);
                    return Err(error(
                        *d,
                        op.clone(),
                        offset,
                        OpenApiErrorKind::MissingPathParameter((*name).to_owned()),
                    ));
                }
            }
        }
        Ok(())
    }

    fn finish(mut self) -> R<OpenApiReading> {
        let builder = self.builder.take().ok_or_else(|| {
            error(
                0,
                String::new(),
                0,
                OpenApiErrorKind::Build("no builder".into()),
            )
        })?;
        let schema = builder
            .build()
            .map_err(|e| error(0, String::new(), 0, OpenApiErrorKind::Build(e.to_string())))?;
        Ok(OpenApiReading {
            schema,
            roots: self.roots,
        })
    }
}

/// Why [`write_openapi`] cannot write a schema.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OpenApiWriteError {
    /// The root, or a vertex an edge reaches, is not in the schema.
    #[error("{0} is not a vertex of the schema")]
    Missing(String),
    /// A constraint whose value is not JSON text of its form.
    #[error("constraint {sort} of {vertex} is not JSON text of its form")]
    Constraint {
        /// The vertex.
        vertex: String,
        /// The constraint's sort.
        sort: String,
    },
    /// A vertex or edge has no place in an `OpenAPI` document.
    #[error("{0} has no place in an OpenAPI document")]
    Structure(String),
    /// A Schema Object cannot be written.
    #[error("Schema Object: {0}")]
    Schema(JsonSchemaWriteError),
}

/// Write the document of an `openapi` schema whose root is `root` as its
/// canonical JSON text.
///
/// The canonical text is one line of JSON with no whitespace between
/// tokens, ended by a line feed. An object's members are its fields in the
/// order of the specification's tables (the protocol's object table): a
/// Path Item's `$ref` first, each value field from its constraint, each
/// field holding objects from its edges (map entries and list items in the
/// order of the edges), an empty map or list from its constraint; then its
/// patterned fields in the order of their edges, a Security Requirement's
/// entries in order; then its specification extensions in the order read.
/// A Reference Object is its `$ref`, `summary`, `description` and the
/// members the specification ignores, in that order. A Schema Object is
/// written as the JSON Schema writer writes a subschema. The `openapi`
/// field is the version read. Reading the text back with [`read_openapi`]
/// gives the schema it was written from.
///
/// # Errors
///
/// Returns an [`OpenApiWriteError`] for a schema no document reads as.
pub fn write_openapi(schema: &Schema, root: &str) -> Result<String, OpenApiWriteError> {
    let mut text = write_object(schema, root)?;
    text.push('\n');
    Ok(text)
}

/// The member `"key":value` a constraint `{"key":value}` holds.
fn member(text: &str) -> Option<&str> {
    text.strip_prefix('{')?.strip_suffix('}')
}

fn write_target(schema: &Schema, id: &str) -> Result<String, OpenApiWriteError> {
    let vertex = schema
        .vertices
        .get(id)
        .ok_or_else(|| OpenApiWriteError::Missing(id.to_owned()))?;
    if SUBSCHEMA_KINDS.contains(&vertex.kind.as_ref()) {
        write_vertex(schema, id).map_err(OpenApiWriteError::Schema)
    } else {
        write_object(schema, id)
    }
}

fn write_object(schema: &Schema, id: &str) -> Result<String, OpenApiWriteError> {
    let vertex = schema
        .vertices
        .get(id)
        .ok_or_else(|| OpenApiWriteError::Missing(id.to_owned()))?;
    let kind: &str = vertex.kind.as_ref();
    let edges = schema.outgoing_edges(id);
    let constraints = schema
        .constraints
        .get(id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let structure = || OpenApiWriteError::Structure(id.to_owned());
    let spliced = |sort: &str| -> Result<Vec<String>, OpenApiWriteError> {
        constraints
            .iter()
            .filter(|c| c.sort.as_ref() == sort)
            .map(|c| {
                member(&c.value)
                    .map(str::to_owned)
                    .ok_or_else(|| OpenApiWriteError::Constraint {
                        vertex: id.to_owned(),
                        sort: sort.to_owned(),
                    })
            })
            .collect()
    };
    let mut members = Vec::new();
    if kind == REFERENCE || kind == "path-item" {
        for edge in edges.iter().filter(|e| e.kind.as_ref() == REF) {
            let text = edge.name.as_deref().ok_or_else(structure)?;
            members.push(format!("\"$ref\":{}", json_string(text)));
        }
    }
    let fields = if kind == REFERENCE {
        REFERENCE_FIELDS
    } else {
        protocol::object(kind).ok_or_else(structure)?.fields
    };
    for field in fields {
        write_field(schema, id, field, edges, constraints, &mut members)?;
    }
    let tail = match kind {
        REFERENCE => spliced(IGNORED)?,
        "security-requirement" => {
            let mut entries = spliced(SCOPES)?;
            entries.extend(spliced(EXTENSION)?);
            entries
        }
        _ => spliced(EXTENSION)?,
    };
    members.extend(tail);
    Ok(format!("{{{}}}", members.join(",")))
}

fn write_field(
    schema: &Schema,
    id: &str,
    field: &Field,
    edges: &[panproto_schema::Edge],
    constraints: &[panproto_schema::Constraint],
    members: &mut Vec<String>,
) -> Result<(), OpenApiWriteError> {
    let key = json_string(field.name);
    if field.holds.is_value() {
        if let Some(constraint) = constraints.iter().find(|c| c.sort.as_ref() == field.name) {
            if serde_json::from_str::<serde::de::IgnoredAny>(&constraint.value).is_err() {
                return Err(OpenApiWriteError::Constraint {
                    vertex: id.to_owned(),
                    sort: field.name.to_owned(),
                });
            }
            members.push(format!("{key}:{}", constraint.value));
        }
        return Ok(());
    }
    let mine: Vec<&panproto_schema::Edge> = edges
        .iter()
        .filter(|e| e.kind.as_ref() == field.edge)
        .collect();
    let named = |edge: &panproto_schema::Edge| -> Result<String, OpenApiWriteError> {
        let name = edge
            .name
            .as_deref()
            .ok_or_else(|| OpenApiWriteError::Structure(id.to_owned()))?;
        Ok(format!(
            "{}:{}",
            json_string(name),
            write_target(schema, edge.tgt.as_ref())?
        ))
    };
    match field.holds {
        Holds::One(_) | Holds::OneOrRef(_) | Holds::Schema => match mine.as_slice() {
            [] => {}
            [edge] => members.push(format!(
                "{key}:{}",
                write_target(schema, edge.tgt.as_ref())?
            )),
            _ => return Err(OpenApiWriteError::Structure(id.to_owned())),
        },
        Holds::List(_) | Holds::ListOrRef(_) => {
            if mine.is_empty() {
                if let Some(constraint) = constraints.iter().find(|c| c.sort.as_ref() == field.name)
                {
                    members.push(format!("{key}:{}", constraint.value));
                }
            } else {
                let items: Result<Vec<String>, _> = mine
                    .iter()
                    .map(|e| write_target(schema, e.tgt.as_ref()))
                    .collect();
                members.push(format!("{key}:[{}]", items?.join(",")));
            }
        }
        Holds::Map(_) | Holds::MapOrRef(_) | Holds::SchemaMap => {
            if mine.is_empty() {
                if let Some(constraint) = constraints.iter().find(|c| c.sort.as_ref() == field.name)
                {
                    members.push(format!("{key}:{}", constraint.value));
                }
            } else {
                let entries: Result<Vec<String>, _> = mine.iter().map(|e| named(e)).collect();
                members.push(format!("{key}:{{{}}}", entries?.join(",")));
            }
        }
        Holds::Patterned { .. } => {
            for edge in mine {
                members.push(named(edge)?);
            }
        }
        _ => {}
    }
    Ok(())
}
