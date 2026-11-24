//! The `AsyncAPI` 3 reader and its canonical writer.
//!
//! [`read_asyncapi`] reads one `AsyncAPI` document, and
//! [`read_asyncapi_bundle`] a description split over several documents,
//! through the shared JSON route ([`crate::config::tree::read_json`], for a
//! document whose first byte other than whitespace is `{`) or the shared
//! YAML route ([`crate::config::tree::read_yaml`], for any other) into a
//! schema of the `asyncapi` protocol, and refuses with an [`AsyncApiError`]
//! naming the document, the JSON pointer and the byte offset of the value
//! at fault every description the reader does not carry exactly.
//! [`write_asyncapi`] writes one document of such a schema back as
//! canonical JSON.
//!
//! The reader reads major version 3 of the specification against the text
//! of 3.1.0: `asyncapi` is a `3.0` or `3.1` version string, and any other
//! version (2.x among them) is refused at that field. Every object is a
//! vertex whose id is the document's prefix (its retrieval URI, or
//! nothing), `#` and the object's JSON pointer, so `#/channels/user` is the
//! channel `user`. The protocol's object table
//! ([`panproto_protocols::api::asyncapi::OBJECTS`]) says how each field is
//! read: a value field becomes a constraint of the field's name holding the
//! value as canonical JSON text, a field holding objects an edge to each.
//! Schema Objects are read by the JSON Schema reader
//! ([`crate::data_schema::json_schema`]) as draft-07 with the `AsyncAPI`
//! vocabulary, and a Multi Format Schema Object's schema likewise when its
//! format is the `AsyncAPI` Schema Object or JSON Schema draft-07; every
//! other format is refused. A Reference Object resolves, as a JSON
//! Reference, against its document's retrieval URI to a place of the
//! bundle; a place of a fragment document is read when a reference first
//! names it, as the kind of object the reference's place takes. A
//! reference to a document outside the bundle is refused, since the reader
//! fetches nothing.

use std::collections::{HashMap, HashSet};
use std::fmt;

use panproto_protocols::api::asyncapi::{
    self as protocol, EXTENSION, Field, Holds, IGNORED, MULTI_FORMAT_SCHEMA, REF, REFERENCE,
    SCHEMA, VOCABULARY,
};
use panproto_protocols::data_schema::json_schema::KINDS as SUBSCHEMA_KINDS;
use panproto_schema::{Schema, SchemaBuilder};

use crate::config::tree::{Content, Node, json_string, read_json, read_yaml};
use crate::data_schema::json_schema::{
    Draft, Embedded, Host, JsonSchemaErrorKind, JsonSchemaWriteError, Vocabulary, build_embedded,
    navigate, pointer_token, write_vertex,
};
use crate::resolution::{resolve_uri_against, resolve_uri_reference};

/// A refusal of the `AsyncAPI` reader: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncApiError {
    /// The index of the document at fault in the bundle, `0` for a single
    /// document.
    pub document: usize,
    /// The JSON pointer of the value at fault from the document root, empty
    /// for the root.
    pub path: String,
    /// The byte offset at which the value starts in its document.
    pub offset: usize,
    /// What is wrong.
    pub kind: AsyncApiErrorKind,
}

impl fmt::Display for AsyncApiError {
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

impl std::error::Error for AsyncApiError {}

/// What an [`AsyncApiError`] refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AsyncApiErrorKind {
    /// The shared JSON or YAML route refuses the document.
    #[error("the {route} route refuses the document: {message}")]
    Syntax {
        /// `JSON` or `YAML`.
        route: &'static str,
        /// The route's refusal.
        message: String,
    },
    /// An `asyncapi` version other than 3.0 and 3.1.
    #[error("asyncapi is \"{0}\": only AsyncAPI 3 (3.0.x and 3.1.x) is read")]
    Version(String),
    /// A document with no `asyncapi` member.
    #[error("not an AsyncAPI document: the asyncapi field is missing")]
    NotAsyncApi,
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
    /// A key outside the pattern its map or object allows: a server name, a
    /// parameter name, a component name, a specification extension.
    #[error("the key \"{name}\" does not match {pattern}")]
    Name {
        /// The key.
        name: String,
        /// The pattern.
        pattern: &'static str,
    },
    /// A Multi Format Schema Object whose `schemaFormat` the reader does
    /// not read.
    #[error(
        "the schema format \"{0}\" is not read; only the AsyncAPI Schema Object and JSON Schema draft-07 are"
    )]
    SchemaFormat(String),
    /// A reference from a place holding an `AsyncAPI` Schema Object to a
    /// Multi Format Schema Object of another format.
    #[error("\"{0}\" names a schema of another schema format")]
    FormatMismatch(String),
    /// A channel address whose expression braces do not pair around a name.
    #[error("the address {0} has an unbalanced or empty expression")]
    Template(String),
    /// An expression of a channel address with no parameter.
    #[error("the address expression {{{0}}} has no parameter")]
    MissingParameter(String),
    /// A parameter whose name is no expression of the channel address.
    #[error("the parameter {0} is not an expression of the address")]
    UnknownParameter(String),
    /// A reference to a document outside the bundle.
    #[error("\"{0}\" names a document outside the bundle, and the reader fetches nothing")]
    External(String),
    /// A reference whose target is no place of the description the reader
    /// reads.
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
    /// A reference of a root channel, operation or reply to a place outside
    /// the root object the specification names.
    #[error("\"{reference}\" breaks the rule that {rule}")]
    Placement {
        /// The reference.
        reference: String,
        /// The rule.
        rule: &'static str,
    },
    /// A message of an operation or reply that is not a message of its
    /// channel.
    #[error("\"{0}\" is not a message of the channel")]
    NotInChannel(String),
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
    /// A complete `AsyncAPI` document, with an `asyncapi` field.
    AsyncApi,
    /// A document holding parts of a description (messages, schemas,
    /// servers, parameters and the like) at JSON pointers that references
    /// name; each place is read when a reference first names it.
    Fragment,
}

/// One document of a bundle: its retrieval URI, if it has one, its bytes
/// and what it is.
#[derive(Clone, Copy, Debug)]
pub struct AsyncApiDocument<'a> {
    /// The absolute URI the document was retrieved from, its base URI.
    pub retrieval_uri: Option<&'a str>,
    /// The JSON or YAML text.
    pub source: &'a [u8],
    /// What the document is.
    pub kind: DocumentKind,
}

/// What the reader reads: the schema, and the vertex of the root of each
/// `AsyncAPI` document, in document order.
#[derive(Clone, Debug)]
pub struct AsyncApiReading {
    /// The schema of the `asyncapi` protocol.
    pub schema: Schema,
    /// The root vertex of each `AsyncAPI` document.
    pub roots: Vec<String>,
}

type R<T> = Result<T, AsyncApiError>;

/// The pattern of a server name and of a parameter name.
const SERVER_NAME: &str = "^[A-Za-z0-9_\\-]+$";

/// The pattern of a component name.
const COMPONENT_NAME: &str = "^[a-zA-Z0-9\\.\\-_]+$";

/// The pattern of a specification extension's name.
const EXTENSION_NAME: &str = "^x-[\\w\\d\\.\\x2d_]+$";

/// Read `source`, one `AsyncAPI` 3 document in JSON or YAML with no
/// retrieval URI, into an `asyncapi` schema.
///
/// # Errors
///
/// Returns an [`AsyncApiError`] naming the place at fault.
pub fn read_asyncapi(source: &[u8]) -> Result<AsyncApiReading, AsyncApiError> {
    read_asyncapi_bundle(&[AsyncApiDocument {
        retrieval_uri: None,
        source,
        kind: DocumentKind::AsyncApi,
    }])
}

/// Read the documents of an `AsyncAPI` description, the first its entry
/// document, into one `asyncapi` schema, resolving references between
/// them.
///
/// The order of the checks, and so which refusal comes first, is the one
/// the book's `AsyncAPI` page states: each document's route, the retrieval
/// URIs, each `AsyncAPI` document's version and objects in document order,
/// the references in the order read (reading each place of a fragment
/// document a reference first names), the Schema Objects, then the rules
/// that join objects (where a root channel's servers, a root operation's
/// channel and a reply's channel are, and which messages an operation and a
/// reply name), object by object in the order read.
///
/// # Errors
///
/// Returns an [`AsyncApiError`] naming the place at fault.
pub fn read_asyncapi_bundle(
    documents: &[AsyncApiDocument<'_>],
) -> Result<AsyncApiReading, AsyncApiError> {
    let mut trees = Vec::with_capacity(documents.len());
    for (i, document) in documents.iter().enumerate() {
        trees.push(read_tree(i, document.source)?);
    }
    let mut reader = Reader::new(documents, &trees)?;
    for i in 0..trees.len() {
        if reader.kinds[i] == DocumentKind::AsyncApi {
            reader.document = i;
            reader.read_document()?;
        }
    }
    reader.resolve_references()?;
    reader.read_schemas()?;
    reader.check_joins()?;
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
    tree.map_err(|e| AsyncApiError {
        document,
        path: String::new(),
        offset: 0,
        kind: AsyncApiErrorKind::Syntax {
            route,
            message: e.to_string(),
        },
    })
}

const fn error(
    document: usize,
    path: String,
    offset: usize,
    kind: AsyncApiErrorKind,
) -> AsyncApiError {
    AsyncApiError {
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

/// Whether `version` is an `AsyncAPI` 3 version string: `3.0.` or `3.1.`
/// followed by a patch number, optionally with a `-` suffix of
/// alphanumeric characters.
fn version_3(version: &str) -> bool {
    ["3.0.", "3.1."].iter().any(|minor| {
        version.strip_prefix(minor).is_some_and(|rest| {
            let (patch, suffix) = rest
                .split_once('-')
                .map_or((rest, None), |(p, s)| (p, Some(s)));
            !patch.is_empty()
                && patch.bytes().all(|b| b.is_ascii_digit())
                && suffix
                    .is_none_or(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric()))
        })
    })
}

/// Whether `name` matches `^[A-Za-z0-9_\-]+$`.
fn server_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// Whether `name` matches `^[a-zA-Z0-9\.\-_]+$`.
fn component_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Whether `name` is a specification extension's name:
/// `^x-[\w\d\.\x2d_]+$`.
fn extension_name(name: &str) -> bool {
    name.strip_prefix("x-").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    })
}

/// Whether `text` begins with a URI scheme and a colon
/// (`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"`), which an absolute
/// URI does and a relative reference does not.
fn absolute(text: &str) -> bool {
    text.split_once(':').is_some_and(|(scheme, _)| {
        let mut bytes = scheme.bytes();
        bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
            && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    })
}

/// Whether `text` names a specific media type: a type and a subtype, each
/// an RFC 6838 restricted name of at most 127 characters, joined by `/`,
/// optionally followed by `;` and parameters, which are not checked.
fn media_type(text: &str) -> bool {
    let essence = text.split_once(';').map_or(text, |(e, _)| e);
    let restricted = |name: &str| {
        let mut bytes = name.bytes();
        name.len() <= 127
            && bytes.next().is_some_and(|b| b.is_ascii_alphanumeric())
            && bytes.all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(
                        b,
                        b'!' | b'#' | b'$' | b'&' | b'-' | b'^' | b'_' | b'.' | b'+'
                    )
            })
    };
    essence
        .split_once('/')
        .is_some_and(|(t, s)| restricted(t) && restricted(s))
}

/// Whether `text` is a runtime expression: `$message.header` or
/// `$message.payload`, optionally followed by `#` and a JSON pointer, in
/// which each `~` is followed by `0` or `1`.
fn runtime_expression(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("$message.") else {
        return false;
    };
    let fragment = ["header", "payload"]
        .iter()
        .find_map(|source| rest.strip_prefix(source));
    match fragment {
        Some("") => true,
        Some(after) => after.strip_prefix('#').is_some_and(|pointer| {
            (pointer.is_empty() || pointer.starts_with('/'))
                && pointer
                    .split('~')
                    .skip(1)
                    .all(|part| part.starts_with('0') || part.starts_with('1'))
        }),
        None => false,
    }
}

/// The expressions of a channel address, in order, or `None` when a brace
/// is unpaired, nested or encloses nothing.
fn expressions(address: &str) -> Option<Vec<&str>> {
    let mut names = Vec::new();
    let mut rest = address;
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

/// How a Multi Format Schema Object's `schemaFormat` reads: `Some(true)`
/// for the `AsyncAPI` Schema Object of a 3.0 or 3.1 version, read with the
/// `AsyncAPI` vocabulary, `Some(false)` for JSON Schema draft-07, read
/// without it, and `None` for any other format.
fn schema_format(text: &str) -> Option<bool> {
    const ASYNCAPI: [&str; 3] = [
        "application/vnd.aai.asyncapi;version=",
        "application/vnd.aai.asyncapi+json;version=",
        "application/vnd.aai.asyncapi+yaml;version=",
    ];
    const DRAFT_07: [&str; 2] = [
        "application/schema+json;version=draft-07",
        "application/schema+yaml;version=draft-07",
    ];
    if ASYNCAPI
        .iter()
        .any(|p| text.strip_prefix(p).is_some_and(version_3))
    {
        Some(true)
    } else if DRAFT_07.contains(&text) {
        Some(false)
    } else {
        None
    }
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

/// Check the value of an `AsyncAPI` vocabulary keyword of a Schema Object:
/// `discriminator` a string, `externalDocs` an External Documentation
/// Object (its `url` an absolute URL), `deprecated` a boolean, which the
/// JSON Schema reader checks.
fn check_vocabulary<'n>(
    key: &str,
    value: &'n Node,
    at: &str,
) -> Result<(), (String, &'n Node, &'static str)> {
    let fail = |path: String, node: &'n Node, rule| Err((path, node, rule));
    match key {
        "discriminator" if value.string().is_none() => {
            fail(at.to_owned(), value, "a discriminator is a string")
        }
        "externalDocs" => {
            let Some(entries) = value.mapping() else {
                return fail(at.to_owned(), value, "externalDocs is an object");
            };
            if get(value, "$ref").is_some() {
                return fail(
                    at.to_owned(),
                    value,
                    "a Reference Object as a Schema Object's externalDocs is not read",
                );
            }
            let Some(url) = get(value, "url") else {
                return fail(at.to_owned(), value, "externalDocs requires url");
            };
            for (k, v) in entries {
                let ok = match k.as_str() {
                    "url" | "description" => v.string().is_some(),
                    other => extension_name(other),
                };
                if !ok {
                    return fail(
                        child(at, k),
                        v,
                        "not a field of an External Documentation Object, or of the wrong type",
                    );
                }
            }
            if url.string().is_some_and(|u| !absolute(u)) {
                return fail(child(at, "url"), url, "url must be an absolute URL");
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The `AsyncAPI` vocabulary of the `AsyncAPI` Schema Object.
const ASYNCAPI_VOCABULARY: Vocabulary = Vocabulary {
    keywords: VOCABULARY,
    check: check_vocabulary,
};

/// A modelled object of the description: the kind its place takes (for a
/// Reference Object, the kind of what it stands for; [`SCHEMA`] for a
/// Schema Object), whether it is a Reference Object, and its node.
struct Slot<'n> {
    kind: &'static str,
    reference: bool,
    node: &'n Node,
}

/// A reference to resolve once every `AsyncAPI` document is read.
#[derive(Clone)]
struct Pending {
    document: usize,
    /// The pointer of the Reference Object.
    pointer: String,
    /// The reference text.
    text: String,
    /// The offset of the reference text.
    offset: usize,
    /// The kind the referenced object must have.
    expected: &'static str,
}

/// An edge added once the Schema Objects are built: from an object to a
/// Schema Object, or from a Reference Object to what it resolves to.
struct LateEdge {
    document: usize,
    at: String,
    offset: usize,
    source: String,
    kind: &'static str,
    name: Option<String>,
    target: String,
}

type Place = (usize, String);

/// A key pattern and the test of a key against it.
type KeyRule = (&'static str, fn(&str) -> bool);

struct Reader<'n> {
    trees: &'n [Node],
    kinds: Vec<DocumentKind>,
    /// Each document's retrieval URI, without an empty fragment.
    uris: Vec<Option<String>>,
    /// Each document's vertex-id prefix.
    prefixes: Vec<String>,
    document: usize,
    builder: Option<SchemaBuilder>,
    slots: HashMap<Place, Slot<'n>>,
    pending: Vec<Pending>,
    schemas: Vec<Embedded<'n>>,
    /// The edges to Schema Objects, in the order met.
    schema_edges: Vec<LateEdge>,
    /// The edges of Reference Objects, in the order resolved.
    ref_edges: Vec<LateEdge>,
    /// Whether each Multi Format Schema Object reads its schema with the
    /// `AsyncAPI` vocabulary (its format is the `AsyncAPI` Schema Object).
    formats: HashMap<Place, bool>,
    /// Every object read, in the order read: its kind, document and
    /// pointer.
    order: Vec<(&'static str, usize, String)>,
    /// Each Reference Object's place to the place it names.
    targets: HashMap<Place, Place>,
    roots: Vec<String>,
}

impl<'n> Reader<'n> {
    fn new(documents: &[AsyncApiDocument<'_>], trees: &'n [Node]) -> R<Self> {
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
                            AsyncApiErrorKind::Invalid(
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
                    AsyncApiErrorKind::DuplicateDocument(uri.unwrap_or_default()),
                ));
            }
            uris.push(uri);
        }
        if documents
            .first()
            .is_some_and(|d| d.kind != DocumentKind::AsyncApi)
        {
            return Err(error(
                0,
                String::new(),
                trees[0].offset,
                AsyncApiErrorKind::Invalid("the entry document must be an AsyncAPI document"),
            ));
        }
        let prefixes = uris.iter().map(|u| u.clone().unwrap_or_default()).collect();
        Ok(Self {
            trees,
            kinds: documents.iter().map(|d| d.kind).collect(),
            uris,
            prefixes,
            document: 0,
            builder: Some(SchemaBuilder::new(&protocol::protocol())),
            slots: HashMap::new(),
            pending: Vec::new(),
            schemas: Vec::new(),
            schema_edges: Vec::new(),
            ref_edges: Vec::new(),
            formats: HashMap::new(),
            order: Vec::new(),
            targets: HashMap::new(),
            roots: Vec::new(),
        })
    }

    const fn fail<T>(&self, path: String, node: &Node, kind: AsyncApiErrorKind) -> R<T> {
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
                AsyncApiErrorKind::Build(e.to_string()),
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
                AsyncApiErrorKind::Unsupported(e.feature()),
            )
        })
    }

    /// Read the `AsyncAPI` document `self.document`: its version and its
    /// objects.
    fn read_document(&mut self) -> R<()> {
        let tree: &'n Node = &self.trees[self.document];
        if tree.mapping().is_none() {
            return self.fail(
                String::new(),
                tree,
                AsyncApiErrorKind::WrongType("an object"),
            );
        }
        let Some(version) = get(tree, "asyncapi") else {
            return self.fail(String::new(), tree, AsyncApiErrorKind::NotAsyncApi);
        };
        match version.string() {
            None => {
                return self.fail(
                    "/asyncapi".into(),
                    version,
                    AsyncApiErrorKind::WrongType("a string"),
                );
            }
            Some(text) if !version_3(text) => {
                return self.fail(
                    "/asyncapi".into(),
                    version,
                    AsyncApiErrorKind::Version(text.to_owned()),
                );
            }
            Some(_) => {}
        }
        let root = self.object("asyncapi", tree, "")?;
        self.with(|b| b.entry(&root));
        self.roots.push(root);
        Ok(())
    }

    /// Whether the place `pointer` of the current document was read, and as
    /// what: `Some(Ok(id))` when it was read as a kind that fits `kind`,
    /// `Some(Err(_))` when as another kind, `None` when it was not read.
    fn read_before(&self, kind: &'static str, node: &Node, pointer: &str) -> Option<R<String>> {
        let slot = self.slots.get(&(self.document, pointer.to_owned()))?;
        Some(if fits(kind, slot.kind) {
            Ok(self.id(pointer))
        } else {
            self.fail(
                pointer.to_owned(),
                node,
                AsyncApiErrorKind::Invalid("a place of a fragment is read as two kinds of object"),
            )
        })
    }

    /// Read the object of kind `kind` at `pointer` (not a Reference Object)
    /// into its vertex, returning the vertex id.
    fn object(&mut self, kind: &'static str, node: &'n Node, pointer: &str) -> R<String> {
        if let Some(read) = self.read_before(kind, node, pointer) {
            return read;
        }
        let Some(entries) = node.mapping() else {
            return self.fail(
                pointer.to_owned(),
                node,
                AsyncApiErrorKind::WrongType("an object"),
            );
        };
        let Some(spec) = protocol::object(kind) else {
            return self.fail(
                pointer.to_owned(),
                node,
                AsyncApiErrorKind::Build(format!("no object kind {kind}")),
            );
        };
        for (key, value) in entries {
            if key.starts_with("x-") {
                if !extension_name(key) {
                    return self.fail(
                        child(pointer, key),
                        value,
                        AsyncApiErrorKind::Name {
                            name: key.clone(),
                            pattern: EXTENSION_NAME,
                        },
                    );
                }
            } else if !spec.fields.iter().any(|f| f.name == key) {
                return self.fail(
                    child(pointer, key),
                    value,
                    AsyncApiErrorKind::UnknownField(key.clone()),
                );
            }
        }
        for field in spec.fields {
            if field.required && get(node, field.name).is_none() {
                return self.fail(
                    pointer.to_owned(),
                    node,
                    AsyncApiErrorKind::Missing(field.name),
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
        for field in spec.fields {
            if let Some(value) = get(node, field.name) {
                self.field(&id, kind, field, value, &child(pointer, field.name))?;
            }
        }
        for (key, value) in entries {
            if key.starts_with("x-") {
                let text = self.json(value, &child(pointer, key))?;
                let member = format!("{{{}:{text}}}", json_string(key));
                self.with(|b| b.constraint(&id, EXTENSION, &member));
            }
        }
        Ok(id)
    }

    /// Read the Reference Object at `pointer`, standing for an object of
    /// kind `kind`: a [`REFERENCE`] vertex, its `$ref` recorded to resolve
    /// and every other member an [`IGNORED`] constraint.
    fn reference(&mut self, kind: &'static str, node: &'n Node, pointer: &str) -> R<String> {
        if let Some(read) = self.read_before(kind, node, pointer) {
            return read;
        }
        let reference = get(node, "$ref").unwrap_or(node);
        let Some(text) = reference.string() else {
            return self.fail(
                child(pointer, "$ref"),
                reference,
                AsyncApiErrorKind::WrongType("a string"),
            );
        };
        self.pending.push(Pending {
            document: self.document,
            pointer: pointer.to_owned(),
            text: text.to_owned(),
            offset: reference.offset,
            expected: kind,
        });
        let id = self.id(pointer);
        self.try_with(pointer, node.offset, |b| b.vertex(&id, REFERENCE, None))?;
        self.slots.insert(
            (self.document, pointer.to_owned()),
            Slot {
                kind,
                reference: true,
                node,
            },
        );
        for (key, value) in node.mapping().unwrap_or_default() {
            if key != "$ref" {
                let text = self.json(value, &child(pointer, key))?;
                let member = format!("{{{}:{text}}}", json_string(key));
                self.with(|b| b.constraint(&id, IGNORED, &member));
            }
        }
        Ok(id)
    }

    /// Read the object at `pointer` that may be a Reference Object to an
    /// object of kind `kind`.
    fn object_or_reference(
        &mut self,
        kind: &'static str,
        node: &'n Node,
        pointer: &str,
    ) -> R<String> {
        if get(node, "$ref").is_some() {
            self.reference(kind, node, pointer)
        } else {
            self.object(kind, node, pointer)
        }
    }

    /// Read the Reference Object at `pointer` of a field that holds only
    /// Reference Objects.
    fn only_reference(&mut self, kind: &'static str, node: &'n Node, pointer: &str) -> R<String> {
        if node.mapping().is_none() {
            return self.fail(
                pointer.to_owned(),
                node,
                AsyncApiErrorKind::WrongType("a Reference Object"),
            );
        }
        if get(node, "$ref").is_none() {
            return self.fail(
                pointer.to_owned(),
                node,
                AsyncApiErrorKind::Invalid("this field holds a Reference Object, not the object"),
            );
        }
        self.reference(kind, node, pointer)
    }

    /// Read the Schema Object or Multi Format Schema Object at `at`, or a
    /// Reference Object to one; a Schema Object is recorded for the JSON
    /// Schema reader, which builds its vertex.
    fn schema_place(&mut self, node: &'n Node, at: &str) -> R<()> {
        if node.mapping().is_some() {
            if get(node, "$ref").is_some() {
                return self.reference(SCHEMA, node, at).map(|_| ());
            }
            if get(node, "schemaFormat").is_some() {
                return self.object(MULTI_FORMAT_SCHEMA, node, at).map(|_| ());
            }
        } else if node.boolean().is_none() {
            return self.fail(
                at.to_owned(),
                node,
                AsyncApiErrorKind::WrongType("a schema"),
            );
        }
        self.embed(node, at, Some(ASYNCAPI_VOCABULARY))
    }

    /// Record the Schema Object at `at`, read as draft-07 with the
    /// vocabulary given, unless the place was read before.
    fn embed(&mut self, node: &'n Node, at: &str, vocabulary: Option<Vocabulary>) -> R<()> {
        if let Some(read) = self.read_before(SCHEMA, node, at) {
            return read.map(|_| ());
        }
        self.slots.insert(
            (self.document, at.to_owned()),
            Slot {
                kind: SCHEMA,
                reference: false,
                node,
            },
        );
        self.schemas.push(Embedded {
            document: self.document,
            pointer: at.to_owned(),
            node,
            vocabulary,
            draft: Draft::Draft07,
        });
        Ok(())
    }

    /// Read a schema field of the object `id`, whose edge is added once the
    /// Schema Objects are built, so the edges of a map of schemas keep the
    /// map's order whatever each entry is.
    fn schema_field(
        &mut self,
        id: &str,
        edge: &'static str,
        name: Option<String>,
        node: &'n Node,
        at: &str,
    ) -> R<()> {
        self.schema_place(node, at)?;
        self.late_schema_edge(id, edge, name, node, at);
        Ok(())
    }

    fn late_schema_edge(
        &mut self,
        id: &str,
        edge: &'static str,
        name: Option<String>,
        node: &Node,
        at: &str,
    ) {
        self.schema_edges.push(LateEdge {
            document: self.document,
            at: at.to_owned(),
            offset: node.offset,
            source: id.to_owned(),
            kind: edge,
            name,
            target: self.id(at),
        });
    }

    /// Read one fixed field of the object `id` of kind `kind`.
    fn field(
        &mut self,
        id: &str,
        kind: &'static str,
        field: &Field,
        value: &'n Node,
        at: &str,
    ) -> R<()> {
        match field.holds {
            Holds::Str
            | Holds::StrOrNull
            | Holds::Any
            | Holds::Strings
            | Holds::StringMap
            | Holds::AnyMap => self.value_field(id, field, value, at),
            Holds::One(target) => {
                let target = self.object(target, value, at)?;
                self.try_with(at, value.offset, |b| b.edge(id, &target, field.edge, None))
            }
            Holds::OneOrRef(target) => {
                let target = self.object_or_reference(target, value, at)?;
                self.try_with(at, value.offset, |b| b.edge(id, &target, field.edge, None))
            }
            Holds::Ref(target) => {
                let target = self.only_reference(target, value, at)?;
                self.try_with(at, value.offset, |b| b.edge(id, &target, field.edge, None))
            }
            Holds::Schema => self.schema_field(id, field.edge, None, value, at),
            Holds::FormatSchema => {
                if value.mapping().is_none() && value.boolean().is_none() {
                    return self.fail(
                        at.to_owned(),
                        value,
                        AsyncApiErrorKind::WrongType("a schema"),
                    );
                }
                let place = (
                    self.document,
                    at.rsplit_once('/').map_or("", |(p, _)| p).to_owned(),
                );
                let vocabulary = self
                    .formats
                    .get(&place)
                    .copied()
                    .unwrap_or_default()
                    .then_some(ASYNCAPI_VOCABULARY);
                self.embed(value, at, vocabulary)?;
                self.late_schema_edge(id, field.edge, None, value, at);
                Ok(())
            }
            Holds::Refs(_) | Holds::List(_) | Holds::ListOrRef(_) => {
                self.list_field(id, field, value, at)
            }
            Holds::MapOrRef(_) | Holds::SchemaMap => self.map_field(id, kind, field, value, at),
        }
    }

    /// Read a field holding a value into a constraint of its name.
    fn value_field(&mut self, id: &str, field: &Field, value: &Node, at: &str) -> R<()> {
        let ok = match field.holds {
            Holds::Str => value.string().is_some(),
            Holds::StrOrNull => {
                value.string().is_some()
                    || matches!(value.content, Content::Scalar(panproto_inst::Value::Null))
            }
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
                Holds::StrOrNull => "a string or null",
                Holds::Strings => "an array of strings",
                Holds::StringMap => "an object of strings",
                _ => "an object",
            };
            return self.fail(at.to_owned(), value, AsyncApiErrorKind::WrongType(expected));
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
                AsyncApiErrorKind::WrongType("an array"),
            );
        };
        if items.is_empty() {
            self.with(|b| b.constraint(id, field.name, "[]"));
        }
        for (i, item) in items.iter().enumerate() {
            let item_at = format!("{at}/{i}");
            let target = match field.holds {
                Holds::Refs(kind) => self.only_reference(kind, item, &item_at)?,
                Holds::ListOrRef(kind) => self.object_or_reference(kind, item, &item_at)?,
                Holds::List(kind) => self.object(kind, item, &item_at)?,
                _ => continue,
            };
            let name = i.to_string();
            self.try_with(&item_at, item.offset, |b| {
                b.edge(id, &target, field.edge, Some(&name))
            })?;
        }
        Ok(())
    }

    /// Read a map field: an edge per entry, named by its key, or the
    /// constraint `{}` for an empty map.
    fn map_field(
        &mut self,
        id: &str,
        kind: &str,
        field: &Field,
        value: &'n Node,
        at: &str,
    ) -> R<()> {
        let Some(entries) = value.mapping() else {
            return self.fail(
                at.to_owned(),
                value,
                AsyncApiErrorKind::WrongType("an object"),
            );
        };
        if entries.is_empty() {
            self.with(|b| b.constraint(id, field.name, "{}"));
        }
        let pattern: Option<KeyRule> = match (kind, field.name) {
            ("components", _) => Some((COMPONENT_NAME, component_name)),
            ("asyncapi", "servers") | ("channel", "parameters") => Some((SERVER_NAME, server_name)),
            _ => None,
        };
        for (key, entry) in entries {
            let entry_at = child(at, key);
            if let Some((name, matches)) = pattern
                && !matches(key)
            {
                return self.fail(
                    entry_at,
                    entry,
                    AsyncApiErrorKind::Name {
                        name: key.clone(),
                        pattern: name,
                    },
                );
            }
            let Holds::MapOrRef(target) = field.holds else {
                self.schema_field(id, field.edge, Some(key.clone()), entry, &entry_at)?;
                continue;
            };
            let target = self.object_or_reference(target, entry, &entry_at)?;
            self.try_with(&entry_at, entry.offset, |b| {
                b.edge(id, &target, field.edge, Some(key))
            })?;
        }
        Ok(())
    }
}

/// Whether an object of kind `found` may stand where `expected` is
/// expected: the same kind, or a Multi Format Schema Object where a schema
/// is.
fn fits(expected: &str, found: &str) -> bool {
    expected == found || (expected == SCHEMA && found == MULTI_FORMAT_SCHEMA)
}

/// The security scheme types.
const SCHEME_TYPES: &[&str] = &[
    "userPassword",
    "apiKey",
    "X509",
    "symmetricEncryption",
    "asymmetricEncryption",
    "httpApiKey",
    "http",
    "oauth2",
    "openIdConnect",
    "plain",
    "scramSha256",
    "scramSha512",
    "gssapi",
];

/// The fields of a Security Scheme that apply to some types only.
const SCHEME_FIELDS: &[&str] = &[
    "name",
    "in",
    "scheme",
    "bearerFormat",
    "flows",
    "openIdConnectUrl",
    "scopes",
];

/// The fields that apply to a security scheme type, and those it requires.
fn scheme_fields(kind: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match kind {
        "apiKey" => (&["in"], &["in"]),
        "httpApiKey" => (&["name", "in"], &["name", "in"]),
        "http" => (&["scheme", "bearerFormat"], &["scheme"]),
        "oauth2" => (&["flows", "scopes"], &["flows"]),
        "openIdConnect" => (&["openIdConnectUrl", "scopes"], &["openIdConnectUrl"]),
        _ => (&[], &[]),
    }
}

/// The fields of each object kind that hold an absolute URL.
fn url_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "asyncapi" => &["id"],
        "info" => &["termsOfService"],
        "contact" | "license" | "external-docs" => &["url"],
        "security-scheme" => &["openIdConnectUrl"],
        "oauth-flow" => &["authorizationUrl", "tokenUrl", "refreshUrl"],
        _ => &[],
    }
}

impl Reader<'_> {
    /// The rules of one object that its field table does not state.
    fn check_object(&mut self, kind: &str, node: &Node, pointer: &str) -> R<()> {
        for name in url_fields(kind) {
            if let Some(value) = get(node, name)
                && value.string().is_some_and(|u| !absolute(u))
            {
                return self.fail(
                    child(pointer, name),
                    value,
                    AsyncApiErrorKind::Invalid("this field must be an absolute URL"),
                );
            }
        }
        let media = match kind {
            "asyncapi" => Some("defaultContentType"),
            "message" | "message-trait" => Some("contentType"),
            _ => None,
        };
        if let Some(name) = media
            && let Some(value) = get(node, name)
            && value.string().is_some_and(|t| !media_type(t))
        {
            return self.fail(
                child(pointer, name),
                value,
                AsyncApiErrorKind::Invalid("a content type must be a specific media type"),
            );
        }
        match kind {
            "channel" => self.check_channel(node, pointer),
            "operation" => match get(node, "action") {
                Some(action)
                    if action
                        .string()
                        .is_some_and(|a| !matches!(a, "send" | "receive")) =>
                {
                    self.fail(
                        child(pointer, "action"),
                        action,
                        AsyncApiErrorKind::Invalid("action is send or receive"),
                    )
                }
                _ => Ok(()),
            },
            "operation-reply-address" | "correlation-id" | "parameter" => {
                match get(node, "location") {
                    Some(location)
                        if location.string().is_some_and(|l| !runtime_expression(l)) =>
                    {
                        self.fail(
                            child(pointer, "location"),
                            location,
                            AsyncApiErrorKind::Invalid(
                                "location must be a runtime expression $message.header or $message.payload, with an optional #JSON pointer",
                            ),
                        )
                    }
                    _ => Ok(()),
                }
            }
            "message-example" => {
                if get(node, "headers").is_none() && get(node, "payload").is_none() {
                    return self.fail(
                        pointer.to_owned(),
                        node,
                        AsyncApiErrorKind::Missing("headers or payload"),
                    );
                }
                Ok(())
            }
            MULTI_FORMAT_SCHEMA => {
                let Some(format) = get(node, "schemaFormat") else {
                    return Ok(());
                };
                let Some(text) = format.string() else {
                    return Ok(());
                };
                let Some(vocabulary) = schema_format(text) else {
                    return self.fail(
                        child(pointer, "schemaFormat"),
                        format,
                        AsyncApiErrorKind::SchemaFormat(text.to_owned()),
                    );
                };
                self.formats
                    .insert((self.document, pointer.to_owned()), vocabulary);
                Ok(())
            }
            "security-scheme" => self.check_scheme(node, pointer),
            "oauth-flows" => self.check_flows(node, pointer),
            _ => Ok(()),
        }
    }

    /// A channel's address has no query or fragment and well-formed
    /// expressions, and its parameters are present only beside expressions
    /// and name exactly its expressions.
    fn check_channel(&self, node: &Node, pointer: &str) -> R<()> {
        let address = get(node, "address");
        let text = address.and_then(Node::string);
        let names = if let (Some(address), Some(text)) = (address, text) {
            let at = child(pointer, "address");
            if text.contains(['?', '#']) {
                return self.fail(
                    at,
                    address,
                    AsyncApiErrorKind::Invalid("a channel address has no query or fragment"),
                );
            }
            let Some(found) = expressions(text) else {
                return self.fail(at, address, AsyncApiErrorKind::Template(text.to_owned()));
            };
            found
        } else {
            Vec::new()
        };
        let Some(parameters) = get(node, "parameters") else {
            return Ok(());
        };
        let at = child(pointer, "parameters");
        if names.is_empty() {
            return self.fail(
                at,
                parameters,
                AsyncApiErrorKind::Invalid(
                    "parameters is present only when the address holds expressions",
                ),
            );
        }
        let Some(entries) = parameters.mapping() else {
            return Ok(());
        };
        for name in &names {
            if !entries.iter().any(|(k, _)| k == name) {
                return self.fail(
                    at,
                    parameters,
                    AsyncApiErrorKind::MissingParameter((*name).to_owned()),
                );
            }
        }
        for (key, value) in entries {
            if !names.contains(&key.as_str()) {
                return self.fail(
                    child(&at, key),
                    value,
                    AsyncApiErrorKind::UnknownParameter(key.clone()),
                );
            }
        }
        Ok(())
    }

    fn check_scheme(&self, node: &Node, pointer: &str) -> R<()> {
        let Some(kind) = get(node, "type").and_then(Node::string) else {
            return Ok(());
        };
        if !SCHEME_TYPES.contains(&kind) {
            return self.fail(
                child(pointer, "type"),
                get(node, "type").unwrap_or(node),
                AsyncApiErrorKind::Invalid("type is not a security scheme type of AsyncAPI 3"),
            );
        }
        let (allowed, required) = scheme_fields(kind);
        for name in SCHEME_FIELDS {
            if let Some(value) = get(node, name)
                && !allowed.contains(name)
            {
                return self.fail(
                    child(pointer, name),
                    value,
                    AsyncApiErrorKind::Invalid("this field does not apply to the scheme's type"),
                );
            }
        }
        for name in required {
            if get(node, name).is_none() {
                return self.fail(pointer.to_owned(), node, AsyncApiErrorKind::Missing(name));
            }
        }
        if let Some(location) = get(node, "in")
            && let Some(text) = location.string()
        {
            let ok = match kind {
                "apiKey" => matches!(text, "user" | "password"),
                _ => matches!(text, "query" | "header" | "cookie"),
            };
            if !ok {
                return self.fail(
                    child(pointer, "in"),
                    location,
                    AsyncApiErrorKind::Invalid(
                        "an API key is in user or password, an HTTP API key in query, header or cookie",
                    ),
                );
            }
        }
        if let Some(format) = get(node, "bearerFormat")
            && !get(node, "scheme")
                .and_then(Node::string)
                .is_some_and(|s| s.eq_ignore_ascii_case("bearer"))
        {
            return self.fail(
                child(pointer, "bearerFormat"),
                format,
                AsyncApiErrorKind::Invalid("bearerFormat applies only to the bearer scheme"),
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
                    return self.fail(at, value, AsyncApiErrorKind::Missing(name));
                }
            }
            if let Some(extra) = get(value, forbidden) {
                return self.fail(
                    child(&at, forbidden),
                    extra,
                    AsyncApiErrorKind::Invalid("this URL does not apply to the flow"),
                );
            }
        }
        Ok(())
    }

    /// The document and JSON pointer a reference made in `document` names.
    fn locate(&self, document: usize, text: &str) -> Result<Place, AsyncApiErrorKind> {
        let (target, fragment) = if let Some(fragment) = text.strip_prefix('#') {
            (document, fragment.to_owned())
        } else {
            let resolved = self.uris[document]
                .as_ref()
                .map_or_else(
                    || resolve_uri_against(None, text),
                    |base| resolve_uri_reference(base, text),
                )
                .ok_or_else(|| AsyncApiErrorKind::External(text.to_owned()))?;
            let (uri, fragment) = resolved.split_once('#').unwrap_or((&resolved, ""));
            let target = self
                .uris
                .iter()
                .position(|u| u.as_deref() == Some(uri))
                .ok_or_else(|| AsyncApiErrorKind::External(text.to_owned()))?;
            (target, fragment.to_owned())
        };
        if !fragment.is_empty() && !fragment.starts_with('/') {
            return Err(AsyncApiErrorKind::RefTarget(text.to_owned()));
        }
        let pointer = percent_decode(&fragment)
            .ok_or_else(|| AsyncApiErrorKind::RefTarget(text.to_owned()))?;
        Ok((target, pointer))
    }

    /// Read the place `place` of a fragment document as an object of kind
    /// `kind`, the first time a reference names it.
    fn read_fragment(&mut self, place: &Place, kind: &'static str) -> Option<R<()>> {
        let (document, pointer) = place;
        if self.kinds[*document] != DocumentKind::Fragment {
            return None;
        }
        let node = navigate(&self.trees[*document], pointer)?;
        self.document = *document;
        Some(if kind == SCHEMA {
            self.schema_place(node, pointer)
        } else {
            self.object_or_reference(kind, node, pointer).map(|_| ())
        })
    }

    /// Resolve each Reference Object, in the order read, to the place it
    /// names, reading a place of a fragment document the first time; check
    /// the kind of what it names; and refuse a cycle of references.
    fn resolve_references(&mut self) -> R<()> {
        let mut next = 0;
        while let Some(reference) = self.pending.get(next).cloned() {
            next += 1;
            let at = child(&reference.pointer, "$ref");
            let fail = |kind| {
                Err(error(
                    reference.document,
                    at.clone(),
                    reference.offset,
                    kind,
                ))
            };
            let place = match self.locate(reference.document, &reference.text) {
                Ok(place) => place,
                Err(kind) => return fail(kind),
            };
            if !self.slots.contains_key(&place)
                && let Some(read) = self.read_fragment(&place, reference.expected)
            {
                read?;
            }
            let Some(slot) = self.slots.get(&place) else {
                return fail(AsyncApiErrorKind::RefTarget(reference.text.clone()));
            };
            if !fits(reference.expected, slot.kind) {
                return fail(AsyncApiErrorKind::RefKind {
                    reference: reference.text.clone(),
                    expected: reference.expected,
                    found: slot.kind,
                });
            }
            if slot.kind == MULTI_FORMAT_SCHEMA && self.formats.get(&place) == Some(&false) {
                return fail(AsyncApiErrorKind::FormatMismatch(reference.text.clone()));
            }
            self.ref_edges.push(LateEdge {
                document: reference.document,
                at: at.clone(),
                offset: reference.offset,
                source: format!(
                    "{}#{}",
                    self.prefixes[reference.document], reference.pointer
                ),
                kind: REF,
                name: Some(reference.text.clone()),
                target: format!("{}#{}", self.prefixes[place.0], place.1),
            });
            self.targets
                .insert((reference.document, reference.pointer.clone()), place);
        }
        for reference in &self.pending {
            let start = (reference.document, reference.pointer.clone());
            let mut seen = HashSet::from([start.clone()]);
            let mut place = start;
            while let Some(next) = self.targets.get(&place) {
                if !seen.insert(next.clone()) {
                    return Err(error(
                        reference.document,
                        child(&reference.pointer, "$ref"),
                        reference.offset,
                        AsyncApiErrorKind::RefCycle(reference.text.clone()),
                    ));
                }
                place = next.clone();
            }
        }
        Ok(())
    }

    /// Build the Schema Objects with the JSON Schema reader, then the edges
    /// to them and the edges of the Reference Objects.
    fn read_schemas(&mut self) -> R<()> {
        let host = Host {
            documents: self.uris.iter().map(Option::as_deref).collect(),
            fragments: self
                .trees
                .iter()
                .zip(&self.kinds)
                .map(|(tree, kind)| (*kind == DocumentKind::Fragment).then_some(tree))
                .collect(),
            schemas: std::mem::take(&mut self.schemas),
            dialects: &[],
        };
        let Some(builder) = self.builder.take() else {
            return Ok(());
        };
        let (builder, _) = build_embedded(&host, builder).map_err(|e| {
            error(
                e.document,
                e.path,
                e.offset,
                AsyncApiErrorKind::Schema(e.kind),
            )
        })?;
        self.builder = Some(builder);
        let edges = std::mem::take(&mut self.schema_edges)
            .into_iter()
            .chain(std::mem::take(&mut self.ref_edges));
        for edge in edges {
            self.document = edge.document;
            self.try_with(&edge.at, edge.offset, |b| {
                b.edge(&edge.source, &edge.target, edge.kind, edge.name.as_deref())
            })?;
        }
        Ok(())
    }

    /// The place a chain of references starting at `place` ends at.
    fn resolved(&self, place: &Place) -> Place {
        let mut place = place.clone();
        while self.slots.get(&place).is_some_and(|s| s.reference) {
            match self.targets.get(&place) {
                Some(next) => place = next.clone(),
                None => break,
            }
        }
        place
    }

    /// The reference text and offset of the Reference Object at `place`.
    fn reference_text(&self, place: &Place) -> (String, usize) {
        self.slots
            .get(place)
            .and_then(|s| get(s.node, "$ref"))
            .map_or((String::new(), 0), |r| {
                (r.string().unwrap_or_default().to_owned(), r.offset)
            })
    }

    /// Whether the object at `place` sits in a root object of its
    /// `AsyncAPI` document: at `/{root}/{key}`, with `suffix` after it.
    fn in_root(&self, place: &Place, root: &str, suffix: &str) -> bool {
        self.kinds[place.0] == DocumentKind::AsyncApi
            && place
                .1
                .strip_prefix(&format!("/{root}/"))
                .and_then(|rest| rest.strip_suffix(suffix))
                .is_some_and(|key| !key.is_empty() && !key.contains('/'))
    }

    /// The rules that join objects, object by object in the order read: a
    /// channel of the root Channels Object names servers of the root
    /// Servers Object; an operation of the root Operations Object, and the
    /// reply inside one, name a channel of the root Channels Object; the
    /// channel of a reply with an address has no address; and the messages
    /// of an operation or reply are messages of its channel.
    fn check_joins(&self) -> R<()> {
        for (kind, document, pointer) in &self.order {
            let here = (*document, pointer.clone());
            match *kind {
                "channel" if self.in_root(&here, "channels", "") => {
                    let node = self.slots[&here].node;
                    for i in 0..get(node, "servers")
                        .and_then(Node::sequence)
                        .map_or(0, <[Node]>::len)
                    {
                        let item = (*document, format!("{pointer}/servers/{i}"));
                        self.check_root_target(&item, "servers", "a channel of the root Channels Object names servers of the root Servers Object")?;
                    }
                }
                "operation" => {
                    let root = self.in_root(&here, "operations", "");
                    self.check_channel_join(&here, root, false)?;
                }
                "operation-reply" => {
                    let root = self.in_root(&here, "operations", "/reply");
                    self.check_channel_join(&here, root, true)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The Reference Object at `item` names a place `/{root}/{key}` of its
    /// own document.
    fn check_root_target(&self, item: &Place, root: &str, rule: &'static str) -> R<()> {
        let Some(target) = self.targets.get(item) else {
            return Ok(());
        };
        if target.0 == item.0 && self.in_root(target, root, "") {
            return Ok(());
        }
        let (reference, offset) = self.reference_text(item);
        Err(error(
            item.0,
            child(&item.1, "$ref"),
            offset,
            AsyncApiErrorKind::Placement { reference, rule },
        ))
    }

    /// The rules of an operation or reply at `here` and its channel.
    fn check_channel_join(&self, here: &Place, root: bool, reply: bool) -> R<()> {
        let (document, pointer) = here;
        let node = self.slots[here].node;
        let channel = (*document, child(pointer, "channel"));
        if !self.slots.contains_key(&channel) {
            return Ok(());
        }
        if root {
            self.check_root_target(
                &channel,
                "channels",
                "an operation of the root Operations Object, and its reply, name a channel of the root Channels Object",
            )?;
        }
        let target = self.resolved(&channel);
        let Some(slot) = self.slots.get(&target) else {
            return Ok(());
        };
        if reply
            && let Some(address) = get(node, "address")
            && get(slot.node, "address")
                .is_some_and(|a| !matches!(a.content, Content::Scalar(panproto_inst::Value::Null)))
        {
            return Err(error(
                *document,
                child(pointer, "address"),
                address.offset,
                AsyncApiErrorKind::Invalid(
                    "a reply with an address names a channel whose address is null or absent",
                ),
            ));
        }
        let count = get(node, "messages")
            .and_then(Node::sequence)
            .map_or(0, <[Node]>::len);
        let messages = format!("{}/messages/", target.1);
        for i in 0..count {
            let item = (*document, format!("{pointer}/messages/{i}"));
            let Some(named) = self.targets.get(&item) else {
                continue;
            };
            let inside = named.0 == target.0
                && named
                    .1
                    .strip_prefix(&messages)
                    .is_some_and(|key| !key.is_empty() && !key.contains('/'));
            if !inside {
                let (reference, offset) = self.reference_text(&item);
                return Err(error(
                    item.0,
                    child(&item.1, "$ref"),
                    offset,
                    AsyncApiErrorKind::NotInChannel(reference),
                ));
            }
        }
        Ok(())
    }

    fn finish(mut self) -> R<AsyncApiReading> {
        let builder = self.builder.take().ok_or_else(|| {
            error(
                0,
                String::new(),
                0,
                AsyncApiErrorKind::Build("no builder".into()),
            )
        })?;
        let schema = builder
            .build()
            .map_err(|e| error(0, String::new(), 0, AsyncApiErrorKind::Build(e.to_string())))?;
        Ok(AsyncApiReading {
            schema,
            roots: self.roots,
        })
    }
}

/// Why [`write_asyncapi`] cannot write a schema.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AsyncApiWriteError {
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
    /// A vertex or edge has no place in an `AsyncAPI` document.
    #[error("{0} has no place in an AsyncAPI document")]
    Structure(String),
    /// A Schema Object cannot be written.
    #[error("Schema Object: {0}")]
    Schema(JsonSchemaWriteError),
}

/// Write the document of an `asyncapi` schema whose root is `root` as its
/// canonical JSON text.
///
/// The canonical text is one line of JSON with no whitespace between
/// tokens, ended by a line feed. An object's members are its fields in the
/// order of the specification's tables (the protocol's object table): each
/// value field from its constraint, each field holding objects from its
/// edges (map entries and list items in the order of the edges), an empty
/// map or list from its constraint; then its specification extensions in
/// the order read. A Reference Object is its `$ref` and then the members
/// the specification ignores, in the order read. A Schema Object is
/// written as the JSON Schema writer writes a subschema. Reading the text
/// back with [`read_asyncapi`] gives the schema it was written from.
///
/// # Errors
///
/// Returns an [`AsyncApiWriteError`] for a schema no document reads as.
pub fn write_asyncapi(schema: &Schema, root: &str) -> Result<String, AsyncApiWriteError> {
    let mut text = write_object(schema, root)?;
    text.push('\n');
    Ok(text)
}

/// The member `"key":value` a constraint `{"key":value}` holds.
fn member(text: &str) -> Option<&str> {
    text.strip_prefix('{')?.strip_suffix('}')
}

fn write_target(schema: &Schema, id: &str) -> Result<String, AsyncApiWriteError> {
    let vertex = schema
        .vertices
        .get(id)
        .ok_or_else(|| AsyncApiWriteError::Missing(id.to_owned()))?;
    if SUBSCHEMA_KINDS.contains(&vertex.kind.as_ref()) {
        write_vertex(schema, id).map_err(AsyncApiWriteError::Schema)
    } else {
        write_object(schema, id)
    }
}

fn write_object(schema: &Schema, id: &str) -> Result<String, AsyncApiWriteError> {
    let vertex = schema
        .vertices
        .get(id)
        .ok_or_else(|| AsyncApiWriteError::Missing(id.to_owned()))?;
    let kind: &str = vertex.kind.as_ref();
    let edges = schema.outgoing_edges(id);
    let constraints = schema
        .constraints
        .get(id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let structure = || AsyncApiWriteError::Structure(id.to_owned());
    let spliced = |sort: &str| -> Result<Vec<String>, AsyncApiWriteError> {
        constraints
            .iter()
            .filter(|c| c.sort.as_ref() == sort)
            .map(|c| {
                member(&c.value)
                    .map(str::to_owned)
                    .ok_or_else(|| AsyncApiWriteError::Constraint {
                        vertex: id.to_owned(),
                        sort: sort.to_owned(),
                    })
            })
            .collect()
    };
    let mut members = Vec::new();
    if kind == REFERENCE {
        let [edge] = edges else {
            return Err(structure());
        };
        let text = edge.name.as_deref().ok_or_else(structure)?;
        members.push(format!("\"$ref\":{}", json_string(text)));
        members.extend(spliced(IGNORED)?);
        return Ok(format!("{{{}}}", members.join(",")));
    }
    let fields = protocol::object(kind).ok_or_else(structure)?.fields;
    for field in fields {
        write_field(schema, id, field, edges, constraints, &mut members)?;
    }
    members.extend(spliced(EXTENSION)?);
    Ok(format!("{{{}}}", members.join(",")))
}

fn write_field(
    schema: &Schema,
    id: &str,
    field: &Field,
    edges: &[panproto_schema::Edge],
    constraints: &[panproto_schema::Constraint],
    members: &mut Vec<String>,
) -> Result<(), AsyncApiWriteError> {
    let key = json_string(field.name);
    let kept = constraints.iter().find(|c| c.sort.as_ref() == field.name);
    if field.holds.is_value() {
        if let Some(constraint) = kept {
            if serde_json::from_str::<serde::de::IgnoredAny>(&constraint.value).is_err() {
                return Err(AsyncApiWriteError::Constraint {
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
    match field.holds {
        Holds::One(_)
        | Holds::OneOrRef(_)
        | Holds::Ref(_)
        | Holds::Schema
        | Holds::FormatSchema => match mine.as_slice() {
            [] => {}
            [edge] => members.push(format!(
                "{key}:{}",
                write_target(schema, edge.tgt.as_ref())?
            )),
            _ => return Err(AsyncApiWriteError::Structure(id.to_owned())),
        },
        Holds::Refs(_) | Holds::List(_) | Holds::ListOrRef(_) => {
            if mine.is_empty() {
                if let Some(constraint) = kept {
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
        Holds::MapOrRef(_) | Holds::SchemaMap => {
            if mine.is_empty() {
                if let Some(constraint) = kept {
                    members.push(format!("{key}:{}", constraint.value));
                }
            } else {
                let mut entries = Vec::with_capacity(mine.len());
                for edge in mine {
                    let name = edge
                        .name
                        .as_deref()
                        .ok_or_else(|| AsyncApiWriteError::Structure(id.to_owned()))?;
                    entries.push(format!(
                        "{}:{}",
                        json_string(name),
                        write_target(schema, edge.tgt.as_ref())?
                    ));
                }
                members.push(format!("{key}:{{{}}}", entries.join(",")));
            }
        }
        _ => {}
    }
    Ok(())
}
