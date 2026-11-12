//! The JSON Schema 2020-12 reader and its canonical writer.
//!
//! [`read_schema`] reads one JSON Schema document, and [`read_bundle`] a set
//! of documents that reference each other, through the shared JSON route
//! ([`crate::config::tree::read_json`]) into a schema of the `json-schema`
//! protocol, and refuses with a [`JsonSchemaError`] naming the place at
//! fault (the document, the JSON pointer and the byte offset) every document
//! the reader does not carry exactly. [`write_document`] writes one document
//! of such a schema back as canonical JSON.
//!
//! The reader follows the 2020-12 dialect
//! (`https://json-schema.org/draft/2020-12/schema`): a document that names
//! another dialect in `$schema` is refused, and one that names none is read
//! as 2020-12. Every subschema is a vertex whose id is the document's prefix
//! (its retrieval URI, else its root `$id`, else empty) followed by `#` and
//! the subschema's JSON pointer from the document root, so `#` is the root
//! of a document without a URI and `#/properties/a` its property `a`. The
//! applicator keywords are edges to the subschemas they hold; every other
//! keyword is a constraint of the vertex holding the keyword's value as
//! canonical JSON text. `$ref` resolves, while the schema is built, against
//! the base URI in scope (RFC 3986, by the shared resolution's URI
//! functions) to a subschema of the bundle; a reference to a resource
//! outside the bundle is refused, since the reader fetches nothing.

use std::collections::{HashMap, HashSet};
use std::fmt;

use panproto_gat::Name;
use panproto_inst::Value;
use panproto_protocols::data_schema::json_schema as protocol;
use panproto_schema::{Edge, Schema, SchemaBuilder};

use crate::config::tree::{Content, JsonTextError, Node, json_string, read_json};
use crate::resolution::{resolve_uri_against, resolve_uri_reference};

pub use protocol::DIALECT;

/// A refusal of the JSON Schema reader: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonSchemaError {
    /// The index of the document at fault in the bundle, `0` for a single
    /// document.
    pub document: usize,
    /// The JSON pointer of the value at fault from the document root, empty
    /// for the root.
    pub path: String,
    /// The byte offset at which the value starts in its document.
    pub offset: usize,
    /// What is wrong.
    pub kind: JsonSchemaErrorKind,
}

impl fmt::Display for JsonSchemaError {
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

impl std::error::Error for JsonSchemaError {}

/// What a [`JsonSchemaError`] refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum JsonSchemaErrorKind {
    /// The shared JSON route refuses the document.
    #[error("the JSON route refuses the document: {0}")]
    Json(String),
    /// A value of another JSON kind than the keyword takes.
    #[error("expected {0}")]
    WrongType(&'static str),
    /// A value that breaks a rule of the 2020-12 meta-schema, which the
    /// message states.
    #[error("{0}")]
    Invalid(&'static str),
    /// `$schema` names a dialect other than 2020-12.
    #[error(
        "$schema names the dialect {0:?}; only https://json-schema.org/draft/2020-12/schema is read"
    )]
    UnsupportedDialect(String),
    /// A keyword of an earlier draft that 2020-12 replaced.
    #[error("{0} is a keyword of an earlier draft that 2020-12 replaced")]
    Superseded(&'static str),
    /// A `$dynamicRef` whose target depends on the dynamic scope of an
    /// evaluation: its static target holds a `$dynamicAnchor` of the name it
    /// gives, and more than one resource of the bundle declares that name.
    #[error("$dynamicRef to {0:?} resolves only against the dynamic scope of an evaluation")]
    DynamicScope(String),
    /// Two resources of the bundle have the same URI.
    #[error("the resource {0:?} is identified twice")]
    DuplicateId(String),
    /// A resource declares the same anchor twice.
    #[error("the anchor {0:?} is declared twice in its resource")]
    DuplicateAnchor(String),
    /// A relative `$id` or `$ref` with no base URI in scope to resolve
    /// against.
    #[error("{0:?} is relative and no base URI is in scope")]
    NoBase(String),
    /// A reference to a resource outside the bundle.
    #[error("{0:?} names a resource outside the bundle, and the reader fetches nothing")]
    RemoteRef(String),
    /// A reference whose JSON pointer names no subschema.
    #[error("{0:?} names no subschema")]
    RefTarget(String),
    /// A reference to an anchor its resource does not declare.
    #[error("{0:?} names an anchor its resource does not declare")]
    UnknownAnchor(String),
    /// Two documents of the bundle with the same retrieval URI, or two
    /// without a URI.
    #[error("the document {0:?} is given twice")]
    DuplicateDocument(String),
    /// A value the canonical JSON text cannot carry.
    #[error("{0} is not supported")]
    Unsupported(&'static str),
    /// The schema the documents describe cannot be built.
    #[error("the schema cannot be built: {0}")]
    Schema(String),
}

/// One document of a bundle: its retrieval URI, if it has one, and its
/// bytes.
#[derive(Clone, Copy, Debug)]
pub struct BundleDocument<'a> {
    /// The absolute URI the document was retrieved from, the initial base
    /// URI of its root.
    pub retrieval_uri: Option<&'a str>,
    /// The JSON text.
    pub source: &'a [u8],
}

/// What the reader reads: the schema, and the vertex of each document's
/// root, in document order.
#[derive(Clone, Debug)]
pub struct JsonSchemaReading {
    /// The schema of the `json-schema` protocol.
    pub schema: Schema,
    /// The root vertex of each document.
    pub roots: Vec<String>,
}

type R<T> = Result<T, JsonSchemaError>;

/// The keywords of an earlier draft that 2020-12 replaced.
const SUPERSEDED: &[&str] = &[
    "definitions",
    "dependencies",
    "additionalItems",
    "$recursiveRef",
    "$recursiveAnchor",
];

/// The seven type names.
const TYPES: &[&str] = &[
    "null", "boolean", "object", "array", "number", "string", "integer",
];

/// How an applicator keyword holds its subschemas.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Holds {
    /// One subschema.
    One,
    /// An object whose values are subschemas, each edge named by its key.
    Map,
    /// A non-empty array of subschemas, each edge named by its index.
    List,
}

/// Each applicator keyword (and `$defs`), how it holds subschemas and the
/// edge kind that reaches them, in the canonical order the writer writes
/// them.
const APPLICATORS: &[(&str, Holds, &str)] = &[
    ("$defs", Holds::Map, "def"),
    ("properties", Holds::Map, "prop"),
    ("patternProperties", Holds::Map, "pattern-prop"),
    ("additionalProperties", Holds::One, "additional-props"),
    ("propertyNames", Holds::One, "property-names"),
    ("dependentSchemas", Holds::Map, "dependent-schema"),
    ("prefixItems", Holds::List, "prefix-item"),
    ("items", Holds::One, "items"),
    ("contains", Holds::One, "contains"),
    ("allOf", Holds::List, "all-of"),
    ("anyOf", Holds::List, "any-of"),
    ("oneOf", Holds::List, "one-of"),
    ("not", Holds::One, "not"),
    ("if", Holds::One, "if"),
    ("then", Holds::One, "then"),
    ("else", Holds::One, "else"),
    ("unevaluatedItems", Holds::One, "unevaluated-items"),
    ("unevaluatedProperties", Holds::One, "unevaluated-props"),
    ("contentSchema", Holds::One, "content-schema"),
];

fn applicator(key: &str) -> Option<(Holds, &'static str)> {
    APPLICATORS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, holds, edge)| (*holds, *edge))
}

const fn error(
    document: usize,
    path: String,
    offset: usize,
    kind: JsonSchemaErrorKind,
) -> JsonSchemaError {
    JsonSchemaError {
        document,
        path,
        offset,
        kind,
    }
}

/// `key` escaped as a JSON pointer reference token (RFC 6901).
#[must_use]
pub fn pointer_token(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn child(pointer: &str, key: &str) -> String {
    format!("{pointer}/{}", pointer_token(key))
}

fn get<'n>(entries: &'n [(String, Node)], key: &str) -> Option<&'n Node> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Whether `text` is an anchor name: `^[A-Za-z_][-A-Za-z0-9._]*$`.
fn anchor_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
}

/// Whether a scalar is a JSON number.
const fn number(value: &Value) -> bool {
    matches!(
        value,
        Value::Int(_) | Value::BigInt(_) | Value::Float(_) | Value::Decimal(_)
    )
}

/// Whether a number is a non-negative integer in the 2020-12 sense: its
/// mathematical value, not its spelling, is a whole number.
fn non_negative_integer(value: &Value) -> bool {
    match value {
        Value::Int(n) => *n >= 0,
        Value::BigInt(n) => !n.negative,
        Value::Float(f) => f.is_finite() && *f >= 0.0 && f.fract().abs().to_bits() == 0,
        Value::Decimal(d) => {
            d.is_finite()
                && (d.is_zero()
                    || (!d.negative
                        && d.exponent.to_i64().map_or_else(
                            || !d.exponent.to_string().starts_with('-'),
                            |e| {
                                e >= 0
                                    || usize::try_from(e.unsigned_abs()).is_ok_and(|n| {
                                        n <= d.digits.len()
                                            && d.digits.bytes().rev().take(n).all(|b| b == b'0')
                                    })
                            },
                        )))
        }
        _ => false,
    }
}

/// Whether a number is greater than zero.
fn positive(value: &Value) -> bool {
    match value {
        Value::Int(n) => *n > 0,
        Value::BigInt(n) => !n.negative,
        Value::Float(f) => *f > 0.0,
        Value::Decimal(d) => d.is_finite() && !d.negative && !d.is_zero(),
        _ => false,
    }
}

/// The check of a vocabulary keyword's value at a JSON pointer: `Ok`, or
/// the pointer of the value at fault, the value and the rule it breaks.
pub(crate) type VocabularyCheck =
    for<'n> fn(&str, &'n Node, &str) -> Result<(), (String, &'n Node, &'static str)>;

/// A vocabulary a host format adds to the 2020-12 dialect: the keywords
/// it defines, which are kept as constraints of their own sort rather than
/// as annotations, and the check of each such keyword's value.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Vocabulary {
    /// The keywords the vocabulary defines.
    pub(crate) keywords: &'static [&'static str],
    /// Checks the value of one of `keywords` at the JSON pointer given,
    /// returning the pointer, the node at fault and the rule it breaks.
    pub(crate) check: VocabularyCheck,
}

/// A dialect a reader accepts in `$schema`: its URI and the vocabulary it
/// adds to 2020-12, if any.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dialect {
    /// The dialect's URI, as `$schema` names it.
    pub(crate) id: &'static str,
    /// The keywords the dialect adds to 2020-12.
    pub(crate) vocabulary: Option<Vocabulary>,
}

/// The dialects a plain JSON Schema document may name: 2020-12 alone.
const PLAIN: &[Dialect] = &[Dialect {
    id: DIALECT,
    vocabulary: None,
}];

/// The base URI and resource in scope at a subschema: the resource's key
/// (its absolute URI, or empty for the root of a document without one), the
/// base URI references resolve against, and the vocabulary in effect.
#[derive(Clone, Debug)]
struct Scope {
    resource: String,
    base: Option<String>,
    vocabulary: Option<Vocabulary>,
}

/// What the first pass learns of the bundle.
#[derive(Default)]
struct Index {
    /// Each resource key, to its document and the pointer of its root.
    resources: HashMap<String, (usize, String)>,
    /// Each `(resource, anchor)`, to the anchored subschema's document,
    /// pointer, and whether the anchor is a `$dynamicAnchor`.
    anchors: HashMap<(String, String), (usize, String, bool)>,
    /// The number of resources declaring each `$dynamicAnchor` name.
    dynamic: HashMap<String, usize>,
    /// The scope at each subschema, by document and pointer.
    scopes: HashMap<(usize, String), Scope>,
    /// Each document's vertex-id prefix.
    prefixes: Vec<String>,
}

/// Read `source`, one JSON Schema document with no retrieval URI, into a
/// `json-schema` schema.
///
/// # Errors
///
/// Returns a [`JsonSchemaError`] naming the place at fault.
pub fn read_schema(source: &[u8]) -> Result<JsonSchemaReading, JsonSchemaError> {
    read_bundle(&[BundleDocument {
        retrieval_uri: None,
        source,
    }])
}

/// Read a bundle of JSON Schema documents into one `json-schema` schema,
/// resolving references between them.
///
/// The documents are read through the JSON route in order, and the first
/// refusal is returned. The order of the checks is the one the book's JSON
/// Schema page states: the first pass checks every keyword of every
/// subschema of every document and registers its resources and anchors;
/// the second builds the vertices and resolves each reference.
///
/// # Errors
///
/// Returns a [`JsonSchemaError`] naming the place at fault.
pub fn read_bundle(documents: &[BundleDocument<'_>]) -> Result<JsonSchemaReading, JsonSchemaError> {
    let mut trees = Vec::with_capacity(documents.len());
    for (i, document) in documents.iter().enumerate() {
        let tree = read_json(document.source).map_err(|e| {
            error(
                i,
                String::new(),
                0,
                JsonSchemaErrorKind::Json(e.to_string()),
            )
        })?;
        trees.push(tree);
    }
    let pairs: Vec<(Option<&str>, &Node)> = documents
        .iter()
        .zip(&trees)
        .map(|(d, t)| (d.retrieval_uri, t))
        .collect();
    read_trees(&pairs)
}

/// Read document trees, each with its retrieval URI, into one
/// `json-schema` schema, as [`read_bundle`] does.
///
/// # Errors
///
/// Returns a [`JsonSchemaError`] naming the place at fault.
pub fn read_trees(
    documents: &[(Option<&str>, &Node)],
) -> Result<JsonSchemaReading, JsonSchemaError> {
    let mut index = Index::default();
    for (i, (uri, root)) in documents.iter().enumerate() {
        let scope = document_scope(i, *uri, root, &mut index)?;
        let mut indexer = Indexer {
            document: i,
            index: &mut index,
            dialects: PLAIN,
            registers_documents: true,
        };
        let prefix = indexer.visit(root, "", &scope, true)?;
        index
            .prefixes
            .push(uri.map_or(prefix, |u| strip_empty_fragment(u).to_owned()));
    }
    let proto = protocol::protocol();
    let mut builder = Builder {
        index: &index,
        schema: Some(SchemaBuilder::new(&proto)),
        refs: Vec::new(),
        document: 0,
    };
    let mut roots = Vec::with_capacity(documents.len());
    for (i, (_, root)) in documents.iter().enumerate() {
        builder.document = i;
        let id = builder.build(root, "")?;
        builder.with(|b| b.entry(&id));
        roots.push(id);
    }
    let schema = builder.finish()?;
    Ok(JsonSchemaReading { schema, roots })
}

/// A Schema Object held by a document of a host format, at a JSON pointer
/// of that document.
pub(crate) struct Embedded<'n> {
    /// The index of the host document.
    pub(crate) document: usize,
    /// The JSON pointer of the Schema Object from the document root, empty
    /// when the whole document is one.
    pub(crate) pointer: String,
    /// The Schema Object.
    pub(crate) node: &'n Node,
}

/// The documents of a host format and the Schema Objects they hold.
pub(crate) struct Host<'a> {
    /// Each document's retrieval URI (absolute, with no fragment, and
    /// distinct, which the host has checked) and the vocabulary its Schema
    /// Objects start with.
    pub(crate) documents: Vec<(Option<&'a str>, Option<Vocabulary>)>,
    /// The Schema Objects, in the order their vertices are built.
    pub(crate) schemas: Vec<Embedded<'a>>,
    /// The dialects `$schema` may name.
    pub(crate) dialects: &'a [Dialect],
}

/// Build the Schema Objects of a host format's documents into `builder`,
/// whose protocol declares the `json-schema` kinds, edges and sorts, and
/// return it with the vertex of each Schema Object, in order.
///
/// Each document is a resource identified by its retrieval URI (or the
/// empty key for the one without), so a `$ref` whose JSON pointer names a
/// Schema Object anywhere in a host document resolves to it, and one that
/// names any other place of the document is refused as naming no
/// subschema. Vertex ids are the document's URI (or nothing), `#` and the
/// JSON pointer, as [`read_bundle`] names them.
pub(crate) fn build_embedded(
    host: &Host<'_>,
    builder: SchemaBuilder,
) -> Result<(SchemaBuilder, Vec<String>), JsonSchemaError> {
    let mut index = Index::default();
    for (i, (uri, _)) in host.documents.iter().enumerate() {
        let key = uri.map_or_else(String::new, |u| strip_empty_fragment(u).to_owned());
        index.resources.insert(key.clone(), (i, String::new()));
        index.prefixes.push(key);
    }
    for schema in &host.schemas {
        let (uri, vocabulary) = host.documents[schema.document];
        let base = uri.map(|u| strip_empty_fragment(u).to_owned());
        let scope = Scope {
            resource: base.clone().unwrap_or_default(),
            base,
            vocabulary,
        };
        let mut indexer = Indexer {
            document: schema.document,
            index: &mut index,
            dialects: host.dialects,
            registers_documents: false,
        };
        indexer.visit(
            schema.node,
            &schema.pointer,
            &scope,
            schema.pointer.is_empty(),
        )?;
    }
    let mut built = Builder {
        index: &index,
        schema: Some(builder),
        refs: Vec::new(),
        document: 0,
    };
    let mut ids = Vec::with_capacity(host.schemas.len());
    for schema in &host.schemas {
        built.document = schema.document;
        ids.push(built.build(schema.node, &schema.pointer)?);
    }
    built.add_refs()?;
    let builder = built.schema.take().ok_or_else(|| {
        error(
            0,
            String::new(),
            0,
            JsonSchemaErrorKind::Schema("no builder".into()),
        )
    })?;
    Ok((builder, ids))
}

/// `uri` without an empty fragment.
fn strip_empty_fragment(uri: &str) -> &str {
    uri.strip_suffix('#').unwrap_or(uri)
}

/// The scope a document's root starts in: its retrieval URI, checked to be
/// absolute and registered, or no base.
fn document_scope(document: usize, uri: Option<&str>, root: &Node, index: &mut Index) -> R<Scope> {
    let Some(uri) = uri else {
        return Ok(Scope {
            resource: String::new(),
            base: None,
            vocabulary: None,
        });
    };
    let base = strip_empty_fragment(uri);
    if base.contains('#') || resolve_uri_against(None, base).is_none() {
        return Err(error(
            document,
            String::new(),
            root.offset,
            JsonSchemaErrorKind::Invalid("a retrieval URI must be absolute, with no fragment"),
        ));
    }
    if index.resources.contains_key(base) {
        return Err(error(
            document,
            String::new(),
            root.offset,
            JsonSchemaErrorKind::DuplicateDocument(base.to_owned()),
        ));
    }
    index
        .resources
        .insert(base.to_owned(), (document, String::new()));
    Ok(Scope {
        resource: base.to_owned(),
        base: Some(base.to_owned()),
        vocabulary: None,
    })
}

/// The first pass over one document.
struct Indexer<'i> {
    document: usize,
    index: &'i mut Index,
    /// The dialects `$schema` may name.
    dialects: &'i [Dialect],
    /// Whether the root of a document without a URI registers the
    /// document's resource (a host format registers its documents itself).
    registers_documents: bool,
}

impl Indexer<'_> {
    const fn fail<T>(&self, path: String, node: &Node, kind: JsonSchemaErrorKind) -> R<T> {
        Err(error(self.document, path, node.offset, kind))
    }

    /// Check and index the subschema `node` at `pointer`, returning the key
    /// of its resource.
    fn visit(&mut self, node: &Node, pointer: &str, parent: &Scope, root: bool) -> R<String> {
        let entries = match &node.content {
            Content::Scalar(Value::Bool(_)) => {
                let scope = self.root_scope(node, pointer, parent, root, None)?;
                let key = scope.resource.clone();
                self.index
                    .scopes
                    .insert((self.document, pointer.to_owned()), scope);
                return Ok(key);
            }
            Content::Mapping(entries) => entries,
            _ => {
                return self.fail(
                    pointer.to_owned(),
                    node,
                    JsonSchemaErrorKind::WrongType("a schema"),
                );
            }
        };
        for (key, value) in entries {
            self.check_keyword(key, value, &child(pointer, key))?;
        }
        let id = get(entries, "$id");
        let vocabulary = if let Some(dialect) = get(entries, "$schema") {
            let at = child(pointer, "$schema");
            if !root && id.is_none() {
                return self.fail(
                    at,
                    dialect,
                    JsonSchemaErrorKind::Invalid(
                        "$schema is allowed only at the root of a schema resource",
                    ),
                );
            }
            let text = dialect.string().unwrap_or_default();
            let Some(named) = self
                .dialects
                .iter()
                .find(|d| d.id == strip_empty_fragment(text))
            else {
                return self.fail(
                    at,
                    dialect,
                    JsonSchemaErrorKind::UnsupportedDialect(text.to_owned()),
                );
            };
            named.vocabulary
        } else {
            parent.vocabulary
        };
        if let Some(vocabulary) = vocabulary {
            for (key, value) in entries {
                if vocabulary.keywords.contains(&key.as_str()) {
                    (vocabulary.check)(key, value, &child(pointer, key)).map_err(
                        |(path, node, rule)| {
                            error(
                                self.document,
                                path,
                                node.offset,
                                JsonSchemaErrorKind::Invalid(rule),
                            )
                        },
                    )?;
                }
            }
        }
        let mut scope = self.root_scope(node, pointer, parent, root, id)?;
        scope.vocabulary = vocabulary;
        for (key, value) in entries {
            if key == "$anchor" || key == "$dynamicAnchor" {
                self.anchor(&scope, key == "$dynamicAnchor", value, pointer)?;
            }
        }
        self.index
            .scopes
            .insert((self.document, pointer.to_owned()), scope.clone());
        for (key, _, _) in APPLICATORS {
            if let Some(value) = get(entries, key) {
                self.visit_children(key, value, pointer, &scope)?;
            }
        }
        Ok(scope.resource)
    }

    /// Visit the subschemas the applicator `key` holds.
    fn visit_children(&mut self, key: &str, value: &Node, pointer: &str, scope: &Scope) -> R<()> {
        let Some((holds, _)) = applicator(key) else {
            return Ok(());
        };
        let at = child(pointer, key);
        match holds {
            Holds::One => {
                self.visit(value, &at, scope, false)?;
            }
            Holds::Map => {
                for (name, sub) in value.mapping().unwrap_or_default() {
                    self.visit(sub, &child(&at, name), scope, false)?;
                }
            }
            Holds::List => {
                for (i, sub) in value.sequence().unwrap_or_default().iter().enumerate() {
                    self.visit(sub, &format!("{at}/{i}"), scope, false)?;
                }
            }
        }
        Ok(())
    }

    /// The scope of a subschema: a new resource for one with `$id`, the
    /// document's own for its root, else the parent's.
    fn root_scope(
        &mut self,
        node: &Node,
        pointer: &str,
        parent: &Scope,
        root: bool,
        id: Option<&Node>,
    ) -> R<Scope> {
        if let Some(id) = id {
            let at = child(pointer, "$id");
            let text = strip_empty_fragment(id.string().unwrap_or_default());
            let Some(resolved) = resolve(parent.base.as_deref(), text) else {
                return self.fail(at, id, JsonSchemaErrorKind::NoBase(text.to_owned()));
            };
            let here = (self.document, pointer.to_owned());
            match self.index.resources.get(&resolved) {
                Some(place) if *place == here => {}
                Some(_) => return self.fail(at, id, JsonSchemaErrorKind::DuplicateId(resolved)),
                None => {}
            }
            self.index.resources.insert(resolved.clone(), here);
            return Ok(Scope {
                resource: resolved.clone(),
                base: Some(resolved),
                vocabulary: parent.vocabulary,
            });
        }
        if root && parent.base.is_none() && self.registers_documents {
            if self.index.resources.contains_key("") {
                return self.fail(
                    String::new(),
                    node,
                    JsonSchemaErrorKind::DuplicateDocument(String::new()),
                );
            }
            self.index
                .resources
                .insert(String::new(), (self.document, String::new()));
        }
        Ok(parent.clone())
    }

    /// Register the anchor `value` of the subschema at `pointer`.
    fn anchor(&mut self, scope: &Scope, dynamic: bool, value: &Node, pointer: &str) -> R<()> {
        let name = value.string().unwrap_or_default().to_owned();
        let keyword = if dynamic { "$dynamicAnchor" } else { "$anchor" };
        let key = (scope.resource.clone(), name.clone());
        if self.index.anchors.contains_key(&key) {
            return self.fail(
                child(pointer, keyword),
                value,
                JsonSchemaErrorKind::DuplicateAnchor(name),
            );
        }
        self.index
            .anchors
            .insert(key, (self.document, pointer.to_owned(), dynamic));
        if dynamic {
            *self.index.dynamic.entry(name).or_default() += 1;
        }
        Ok(())
    }

    /// Check one keyword's value against the 2020-12 meta-schema.
    fn check_keyword(&self, key: &str, value: &Node, at: &str) -> R<()> {
        if let Some(superseded) = SUPERSEDED.iter().find(|s| **s == key) {
            return self.fail(
                at.to_owned(),
                value,
                JsonSchemaErrorKind::Superseded(superseded),
            );
        }
        let scalar = match &value.content {
            Content::Scalar(v) => Some(v),
            _ => None,
        };
        let fail = |kind| self.fail(at.to_owned(), value, kind);
        match key {
            "$schema" | "$ref" | "$dynamicRef" | "$comment" | "title" | "description"
            | "format" | "contentEncoding" | "contentMediaType" | "pattern" => {
                if value.string().is_none() {
                    return fail(JsonSchemaErrorKind::WrongType("a string"));
                }
            }
            "$id" => match value.string() {
                None => return fail(JsonSchemaErrorKind::WrongType("a string")),
                Some(text) if strip_empty_fragment(text).contains('#') => {
                    return fail(JsonSchemaErrorKind::Invalid(
                        "$id must not have a non-empty fragment",
                    ));
                }
                Some(_) => {}
            },
            "$anchor" | "$dynamicAnchor" => match value.string() {
                None => return fail(JsonSchemaErrorKind::WrongType("a string")),
                Some(text) if !anchor_name(text) => {
                    return fail(JsonSchemaErrorKind::Invalid(
                        "an anchor must match ^[A-Za-z_][-A-Za-z0-9._]*$",
                    ));
                }
                Some(_) => {}
            },
            "$vocabulary" => {
                let Some(entries) = value.mapping() else {
                    return fail(JsonSchemaErrorKind::WrongType("an object"));
                };
                for (uri, flag) in entries {
                    if flag.boolean().is_none() {
                        return self.fail(
                            child(at, uri),
                            flag,
                            JsonSchemaErrorKind::WrongType("a boolean"),
                        );
                    }
                }
            }
            "type" => self.check_type(value, at)?,
            "enum" | "examples" if value.sequence().is_none() => {
                return fail(JsonSchemaErrorKind::WrongType("an array"));
            }
            "multipleOf" => match scalar {
                Some(v) if number(v) && positive(v) => {}
                Some(v) if number(v) => {
                    return fail(JsonSchemaErrorKind::Invalid(
                        "multipleOf must be greater than 0",
                    ));
                }
                _ => return fail(JsonSchemaErrorKind::WrongType("a number")),
            },
            "maximum" | "exclusiveMaximum" | "minimum" | "exclusiveMinimum"
                if !scalar.is_some_and(number) =>
            {
                return fail(JsonSchemaErrorKind::WrongType("a number"));
            }
            "maxLength" | "minLength" | "maxItems" | "minItems" | "maxContains" | "minContains"
            | "maxProperties" | "minProperties"
                if !scalar.is_some_and(non_negative_integer) =>
            {
                return fail(JsonSchemaErrorKind::WrongType("a non-negative integer"));
            }
            "uniqueItems" | "deprecated" | "readOnly" | "writeOnly"
                if value.boolean().is_none() =>
            {
                return fail(JsonSchemaErrorKind::WrongType("a boolean"));
            }
            "required" => self.check_names(value, at)?,
            "dependentRequired" => {
                let Some(entries) = value.mapping() else {
                    return fail(JsonSchemaErrorKind::WrongType("an object"));
                };
                for (name, names) in entries {
                    self.check_names(names, &child(at, name))?;
                }
            }
            _ => self.check_applicator(key, value, at)?,
        }
        Ok(())
    }

    /// Check the container of an applicator keyword; its subschemas are
    /// checked when they are visited.
    fn check_applicator(&self, key: &str, value: &Node, at: &str) -> R<()> {
        match applicator(key) {
            Some((Holds::Map, _)) if value.mapping().is_none() => self.fail(
                at.to_owned(),
                value,
                JsonSchemaErrorKind::WrongType("an object"),
            ),
            Some((Holds::List, _)) => match value.sequence() {
                None => self.fail(
                    at.to_owned(),
                    value,
                    JsonSchemaErrorKind::WrongType("an array"),
                ),
                Some([]) => self.fail(
                    at.to_owned(),
                    value,
                    JsonSchemaErrorKind::Invalid("the array must not be empty"),
                ),
                Some(_) => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// Check `type`: one type name, or a non-empty array of distinct ones.
    fn check_type(&self, value: &Node, at: &str) -> R<()> {
        let name_rule = "the type must be null, boolean, object, array, number, string or integer";
        if let Some(text) = value.string() {
            if TYPES.contains(&text) {
                return Ok(());
            }
            return self.fail(
                at.to_owned(),
                value,
                JsonSchemaErrorKind::Invalid(name_rule),
            );
        }
        let Some(items) = value.sequence() else {
            return self.fail(
                at.to_owned(),
                value,
                JsonSchemaErrorKind::WrongType("a string or an array"),
            );
        };
        if items.is_empty() {
            return self.fail(
                at.to_owned(),
                value,
                JsonSchemaErrorKind::Invalid("the array must not be empty"),
            );
        }
        let mut seen = HashSet::new();
        for (i, item) in items.iter().enumerate() {
            let item_at = format!("{at}/{i}");
            let Some(text) = item.string() else {
                return self.fail(item_at, item, JsonSchemaErrorKind::WrongType("a string"));
            };
            if !TYPES.contains(&text) {
                return self.fail(item_at, item, JsonSchemaErrorKind::Invalid(name_rule));
            }
            if !seen.insert(text) {
                return self.fail(
                    item_at,
                    item,
                    JsonSchemaErrorKind::Invalid("the entries must be unique"),
                );
            }
        }
        Ok(())
    }

    /// Check an array of distinct strings (`required`, a value of
    /// `dependentRequired`).
    fn check_names(&self, value: &Node, at: &str) -> R<()> {
        let Some(items) = value.sequence() else {
            return self.fail(
                at.to_owned(),
                value,
                JsonSchemaErrorKind::WrongType("an array"),
            );
        };
        let mut seen = HashSet::new();
        for (i, item) in items.iter().enumerate() {
            let item_at = format!("{at}/{i}");
            let Some(text) = item.string() else {
                return self.fail(item_at, item, JsonSchemaErrorKind::WrongType("a string"));
            };
            if !seen.insert(text) {
                return self.fail(
                    item_at,
                    item,
                    JsonSchemaErrorKind::Invalid("the entries must be unique"),
                );
            }
        }
        Ok(())
    }
}

/// Resolve `reference` against `base`, or, with no base, only an absolute
/// reference.
fn resolve(base: Option<&str>, reference: &str) -> Option<String> {
    base.map_or_else(
        || resolve_uri_against(None, reference),
        |base| resolve_uri_reference(base, reference),
    )
}

/// `fragment` with its `%XX` escapes decoded, or `None` when an escape is
/// malformed or the bytes are not UTF-8.
fn percent_decode(fragment: &str) -> Option<String> {
    let bytes = fragment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(text, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The second pass: builds the schema.
struct Builder<'i> {
    index: &'i Index,
    schema: Option<SchemaBuilder>,
    /// Each reference edge, `(source, target, kind, name)`, added once every
    /// vertex exists.
    refs: Vec<(String, String, &'static str, String)>,
    document: usize,
}

impl Builder<'_> {
    fn with(&mut self, step: impl FnOnce(SchemaBuilder) -> SchemaBuilder) {
        self.schema = self.schema.take().map(step);
    }

    fn try_with(
        &mut self,
        path: &str,
        offset: usize,
        step: impl FnOnce(SchemaBuilder) -> Result<SchemaBuilder, panproto_schema::SchemaError>,
    ) -> R<()> {
        let Some(builder) = self.schema.take() else {
            return Ok(());
        };
        let built = step(builder).map_err(|e| {
            error(
                self.document,
                path.to_owned(),
                offset,
                JsonSchemaErrorKind::Schema(e.to_string()),
            )
        })?;
        self.schema = Some(built);
        Ok(())
    }

    fn vertex_id(&self, pointer: &str) -> String {
        format!("{}#{pointer}", self.index.prefixes[self.document])
    }

    /// Build the vertex of the subschema `node` at `pointer`, with its
    /// constraints and its subschemas, returning its id.
    fn build(&mut self, node: &Node, pointer: &str) -> R<String> {
        let id = self.vertex_id(pointer);
        let entries = match &node.content {
            Content::Scalar(Value::Bool(flag)) => {
                let kind = if *flag { "true" } else { "false" };
                self.try_with(pointer, node.offset, |b| b.vertex(&id, kind, None))?;
                return Ok(id);
            }
            Content::Mapping(entries) => entries,
            _ => {
                return Err(error(
                    self.document,
                    pointer.to_owned(),
                    node.offset,
                    JsonSchemaErrorKind::WrongType("a schema"),
                ));
            }
        };
        let kind = match get(entries, "type").map(|t| &t.content) {
            None => "any",
            Some(Content::Scalar(Value::Str(name))) => TYPES
                .iter()
                .copied()
                .find(|t| *t == name.as_str())
                .unwrap_or("any"),
            Some(_) => "union",
        };
        self.try_with(pointer, node.offset, |b| b.vertex(&id, kind, None))?;
        for (key, value) in entries {
            let empty = matches!(applicator(key), Some((Holds::Map, _)))
                && value.mapping().is_some_and(<[(String, Node)]>::is_empty);
            if (applicator(key).is_some() && !empty) || key == "$ref" || key == "$dynamicRef" {
                continue;
            }
            let at = child(pointer, key);
            let text = self.json(value, &at)?;
            let vocabulary = self.index.scopes[&(self.document, pointer.to_owned())]
                .vocabulary
                .map_or(&[][..], |v| v.keywords);
            if empty || protocol::KEPT.contains(&key.as_str()) || vocabulary.contains(&key.as_str())
            {
                self.with(|b| b.constraint(&id, key, &text));
            } else {
                let annotation = format!("{{{}:{text}}}", json_string(key));
                self.with(|b| b.constraint(&id, protocol::ANNOTATION, &annotation));
            }
        }
        for key in ["$ref", "$dynamicRef"] {
            if let Some(value) = get(entries, key) {
                self.reference(&id, pointer, key, value)?;
            }
        }
        for (key, _, _) in APPLICATORS {
            if let Some(value) = get(entries, key) {
                self.build_children(&id, key, value, pointer)?;
            }
        }
        self.required(&id, entries, pointer);
        Ok(id)
    }

    fn json(&self, node: &Node, at: &str) -> R<String> {
        node.json_text().map_err(|e: JsonTextError| {
            error(
                self.document,
                at.to_owned(),
                node.offset,
                JsonSchemaErrorKind::Unsupported(e.feature()),
            )
        })
    }

    /// Build the subschemas the applicator `key` holds and the edges to them.
    fn build_children(&mut self, id: &str, key: &str, value: &Node, pointer: &str) -> R<()> {
        let Some((holds, edge)) = applicator(key) else {
            return Ok(());
        };
        let at = child(pointer, key);
        let children: Vec<(String, Option<String>, &Node)> = match holds {
            Holds::One => vec![(at, None, value)],
            Holds::Map => value
                .mapping()
                .unwrap_or_default()
                .iter()
                .map(|(name, sub)| (child(&at, name), Some(name.clone()), sub))
                .collect(),
            Holds::List => value
                .sequence()
                .unwrap_or_default()
                .iter()
                .enumerate()
                .map(|(i, sub)| (format!("{at}/{i}"), Some(i.to_string()), sub))
                .collect(),
        };
        for (sub_pointer, name, sub) in children {
            let target = self.build(sub, &sub_pointer)?;
            if key == "$defs" {
                self.with(|b| b.entry(&target));
            }
            self.try_with(&sub_pointer, sub.offset, |b| {
                b.edge(id, &target, edge, name.as_deref())
            })?;
        }
        Ok(())
    }

    /// Declare the required edges of the subschema `id`: each `required`
    /// name that `properties` declares.
    fn required(&mut self, id: &str, entries: &[(String, Node)], pointer: &str) {
        let Some(names) = get(entries, "required").and_then(Node::sequence) else {
            return;
        };
        let declared = get(entries, "properties")
            .and_then(Node::mapping)
            .unwrap_or_default();
        let edges: Vec<Edge> = names
            .iter()
            .filter_map(Node::string)
            .filter(|name| declared.iter().any(|(k, _)| k == name))
            .map(|name| Edge {
                src: Name::from(id),
                tgt: Name::from(self.vertex_id(&format!(
                    "{}/{}",
                    child(pointer, "properties"),
                    pointer_token(name)
                ))),
                kind: Name::from("prop"),
                name: Some(Name::from(name)),
            })
            .collect();
        if !edges.is_empty() {
            self.with(|b| b.required(id, edges));
        }
    }

    /// Resolve the `$ref` or `$dynamicRef` `value` of the subschema at
    /// `pointer` and record its edge.
    fn reference(&mut self, id: &str, pointer: &str, key: &str, value: &Node) -> R<()> {
        let at = child(pointer, key);
        let text = value.string().unwrap_or_default();
        let index = self.index;
        let here = self.document;
        let fail = |kind| Err(error(here, at.clone(), value.offset, kind));
        let scope = &index.scopes[&(here, pointer.to_owned())];
        let (resource, fragment) = if let Some(fragment) = text.strip_prefix('#') {
            (scope.resource.clone(), fragment.to_owned())
        } else {
            let Some(resolved) = resolve(scope.base.as_deref(), text) else {
                return fail(JsonSchemaErrorKind::NoBase(text.to_owned()));
            };
            match resolved.split_once('#') {
                Some((uri, fragment)) => (uri.to_owned(), fragment.to_owned()),
                None => (resolved, String::new()),
            }
        };
        let Some((document, root)) = index.resources.get(&resource) else {
            return fail(JsonSchemaErrorKind::RemoteRef(text.to_owned()));
        };
        let (document, target, dynamic) = if fragment.is_empty() {
            (*document, root.clone(), false)
        } else if fragment.starts_with('/') {
            let Some(decoded) = percent_decode(&fragment) else {
                return fail(JsonSchemaErrorKind::RefTarget(text.to_owned()));
            };
            let target = format!("{root}{decoded}");
            if !index.scopes.contains_key(&(*document, target.clone())) {
                return fail(JsonSchemaErrorKind::RefTarget(text.to_owned()));
            }
            (*document, target, false)
        } else {
            let Some((document, target, dynamic)) =
                index.anchors.get(&(resource.clone(), fragment.clone()))
            else {
                return fail(JsonSchemaErrorKind::UnknownAnchor(text.to_owned()));
            };
            (*document, target.clone(), *dynamic)
        };
        let edge = if key == "$ref" { "ref" } else { "dynamic-ref" };
        if edge == "dynamic-ref"
            && dynamic
            && index.dynamic.get(&fragment).copied().unwrap_or(0) > 1
        {
            return fail(JsonSchemaErrorKind::DynamicScope(fragment));
        }
        let target_id = format!("{}#{target}", index.prefixes[document]);
        self.refs
            .push((id.to_owned(), target_id, edge, text.to_owned()));
        Ok(())
    }

    /// Add the reference edges recorded while the vertices were built.
    fn add_refs(&mut self) -> R<()> {
        for (src, tgt, kind, name) in std::mem::take(&mut self.refs) {
            self.try_with("", 0, |b| b.edge(&src, &tgt, kind, Some(&name)))?;
        }
        Ok(())
    }

    fn finish(mut self) -> R<Schema> {
        self.add_refs()?;
        let builder = self.schema.take().ok_or_else(|| {
            error(
                0,
                String::new(),
                0,
                JsonSchemaErrorKind::Schema("no builder".into()),
            )
        })?;
        builder.build().map_err(|e| {
            error(
                0,
                String::new(),
                0,
                JsonSchemaErrorKind::Schema(e.to_string()),
            )
        })
    }
}

/// Why [`write_document`] cannot write a schema.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum JsonSchemaWriteError {
    /// The root, or a vertex an edge reaches, is not in the schema.
    #[error("{0} is not a vertex of the schema")]
    Missing(String),
    /// A constraint whose value is not JSON text, or an annotation that is
    /// not an object of one member.
    #[error("constraint {sort} of {vertex} is not JSON text of its form")]
    Constraint {
        /// The vertex.
        vertex: String,
        /// The constraint's sort.
        sort: String,
    },
    /// A vertex or edge has no place in a JSON Schema document.
    #[error("{0} has no place in a JSON Schema document")]
    Structure(String),
}

/// Write the document of a `json-schema` schema whose root is `root` as its
/// canonical JSON text.
///
/// The canonical text is one line of JSON with no whitespace between
/// tokens, ended by a line feed. A `true` or `false` vertex is written as
/// that boolean. Any other vertex is an object whose members are its kept
/// constraints in the order the schema holds them (an annotation spliced in
/// as the member it holds, and an empty object of subschemas kept under its
/// applicator's name), then its edges keyword by keyword in this
/// order: `$ref`, `$dynamicRef`, then the applicators `$defs`,
/// `properties`, `patternProperties`, `additionalProperties`,
/// `propertyNames`, `dependentSchemas`, `prefixItems`, `items`, `contains`,
/// `allOf`, `anyOf`, `oneOf`, `not`, `if`, `then`, `else`,
/// `unevaluatedItems`, `unevaluatedProperties` and `contentSchema`, the
/// subschemas of one keyword in the order of their edges. Reading the text
/// back with [`read_schema`], with the document's retrieval URI, gives the
/// schema the text was written from.
///
/// # Errors
///
/// Returns a [`JsonSchemaWriteError`] for a schema no document reads as.
pub fn write_document(schema: &Schema, root: &str) -> Result<String, JsonSchemaWriteError> {
    let mut text = write_vertex(schema, root)?;
    text.push('\n');
    Ok(text)
}

/// The canonical JSON text of the subschema `id`, as [`write_document`]
/// writes it, with no line feed.
pub(crate) fn write_vertex(schema: &Schema, id: &str) -> Result<String, JsonSchemaWriteError> {
    let vertex = schema
        .vertices
        .get(id)
        .ok_or_else(|| JsonSchemaWriteError::Missing(id.to_owned()))?;
    let edges = schema.outgoing_edges(id);
    let constraints = schema
        .constraints
        .get(id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    match vertex.kind.as_ref() {
        "true" | "false" if edges.is_empty() && constraints.is_empty() => {
            return Ok(vertex.kind.to_string());
        }
        "true" | "false" => return Err(JsonSchemaWriteError::Structure(id.to_owned())),
        _ => {}
    }
    let mut members = Vec::new();
    for constraint in constraints {
        let malformed = || JsonSchemaWriteError::Constraint {
            vertex: id.to_owned(),
            sort: constraint.sort.to_string(),
        };
        if constraint.sort.as_ref() == protocol::ANNOTATION {
            members.push(annotation_member(&constraint.value).ok_or_else(malformed)?);
        } else {
            if !is_json(&constraint.value) {
                return Err(malformed());
            }
            members.push(format!(
                "{}:{}",
                json_string(&constraint.sort),
                constraint.value
            ));
        }
    }
    for (keyword, kind) in [("$ref", "ref"), ("$dynamicRef", "dynamic-ref")] {
        for edge in edges.iter().filter(|e| e.kind.as_ref() == kind) {
            let name = edge
                .name
                .as_deref()
                .ok_or_else(|| JsonSchemaWriteError::Structure(id.to_owned()))?;
            members.push(format!("{}:{}", json_string(keyword), json_string(name)));
        }
    }
    for (keyword, holds, kind) in APPLICATORS {
        let mine: Vec<&Edge> = edges.iter().filter(|e| e.kind.as_ref() == *kind).collect();
        if mine.is_empty() {
            continue;
        }
        let value = match holds {
            Holds::One => {
                let [edge] = mine.as_slice() else {
                    return Err(JsonSchemaWriteError::Structure(id.to_owned()));
                };
                let target: &str = edge.tgt.as_ref();
                write_vertex(schema, target)?
            }
            Holds::Map => {
                let mut parts = Vec::new();
                for edge in mine {
                    let name = edge
                        .name
                        .as_deref()
                        .ok_or_else(|| JsonSchemaWriteError::Structure(id.to_owned()))?;
                    let target: &str = edge.tgt.as_ref();
                    parts.push(format!(
                        "{}:{}",
                        json_string(name),
                        write_vertex(schema, target)?
                    ));
                }
                format!("{{{}}}", parts.join(","))
            }
            Holds::List => {
                let mut parts = Vec::new();
                for edge in mine {
                    let target: &str = edge.tgt.as_ref();
                    parts.push(write_vertex(schema, target)?);
                }
                format!("[{}]", parts.join(","))
            }
        };
        members.push(format!("{}:{value}", json_string(keyword)));
    }
    Ok(format!("{{{}}}", members.join(",")))
}

/// Whether `text` is one JSON value.
fn is_json(text: &str) -> bool {
    serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok()
}

/// The member `"key":value` an annotation constraint `{"key":value}` holds.
fn annotation_member(text: &str) -> Option<String> {
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    let mut keys = serde_json::Deserializer::from_str(inner).into_iter::<String>();
    keys.next()?.ok()?;
    let rest = inner.get(keys.byte_offset()..)?.strip_prefix(':')?;
    is_json(rest).then(|| inner.to_owned())
}
