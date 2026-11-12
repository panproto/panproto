//! The `AsyncAPI` page's reference: an `AsyncAPI` 3 description read as the
//! page states, over data trees, with its own field table, and its
//! canonical text.
//!
//! Schema Objects are read by the JSON Schema reference
//! ([`super::json_schema::read_draft07`]) as draft-07, with the `AsyncAPI`
//! vocabulary unless a Multi Format Schema Object names draft-07.

use std::collections::{BTreeMap, BTreeSet};

use super::json_schema::{
    self as js, Doc, Reading, extension, has_scheme, json, json_string, resolve, token,
};

/// A refusal: the document, the JSON pointer of the value at fault, and the
/// refusal's class (the name of the production variant).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The document's index.
    pub document: usize,
    /// The JSON pointer from the document root; empty for the root.
    pub path: String,
    /// The class.
    pub class: &'static str,
}

/// One document of a description: its retrieval URI, its tree, and whether
/// it is a fragment document (else an `AsyncAPI` document).
pub type Document = (Option<String>, Doc, bool);

type R<T> = Result<T, Refusal>;

fn refuse<T>(document: usize, path: &str, class: &'static str) -> R<T> {
    Err(Refusal {
        document,
        path: path.to_owned(),
        class,
    })
}

fn at(path: &str, key: &str) -> String {
    format!("{path}/{}", token(key))
}

fn get<'d>(node: &'d Doc, key: &str) -> Option<&'d Doc> {
    match node {
        Doc::Map(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

/// How a field holds its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum H {
    Str,
    StrNull,
    Any,
    Strs,
    StrMap,
    AnyMap,
    One(&'static str),
    OneRef(&'static str),
    Ref(&'static str),
    Refs(&'static str),
    List(&'static str),
    ListRef(&'static str),
    MapRef(&'static str),
    Schema,
    Schemas,
    Format,
}

/// A field: name, how it holds, whether REQUIRED, and its edge kind.
type F = (&'static str, H, bool, &'static str);

const fn v(name: &'static str, h: H) -> F {
    (name, h, false, "")
}

const fn o(name: &'static str, h: H, edge: &'static str) -> F {
    (name, h, false, edge)
}

const PROTOCOLS: [&str; 20] = [
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

/// The fields of each object kind, in the specification's order.
fn fields(kind: &str) -> Vec<F> {
    [root_fields, message_fields, other_fields]
        .iter()
        .map(|table| table(kind))
        .find(|found| !found.is_empty())
        .unwrap_or_default()
}

fn root_fields(kind: &str) -> Vec<F> {
    let tags = o("tags", H::ListRef("tag"), "tags");
    let docs = o("externalDocs", H::OneRef("external-docs"), "external-docs");
    match kind {
        "asyncapi" => vec![
            ("asyncapi", H::Str, true, ""),
            v("id", H::Str),
            ("info", H::One("info"), true, "info"),
            o("servers", H::MapRef("server"), "servers"),
            v("defaultContentType", H::Str),
            o("channels", H::MapRef("channel"), "channels"),
            o("operations", H::MapRef("operation"), "operations"),
            o("components", H::One("components"), "components"),
        ],
        "info" => vec![
            ("title", H::Str, true, ""),
            ("version", H::Str, true, ""),
            v("description", H::Str),
            v("termsOfService", H::Str),
            o("contact", H::One("contact"), "contact"),
            o("license", H::One("license"), "license"),
            tags,
            docs,
        ],
        "contact" => vec![v("name", H::Str), v("url", H::Str), v("email", H::Str)],
        "license" => vec![("name", H::Str, true, ""), v("url", H::Str)],
        "server" => vec![
            ("host", H::Str, true, ""),
            ("protocol", H::Str, true, ""),
            v("protocolVersion", H::Str),
            v("pathname", H::Str),
            v("description", H::Str),
            v("title", H::Str),
            v("summary", H::Str),
            o("variables", H::MapRef("server-variable"), "variables"),
            o("security", H::ListRef("security-scheme"), "security"),
            tags,
            docs,
            o("bindings", H::OneRef("server-bindings"), "bindings"),
        ],
        "server-variable" | "parameter" => {
            let mut table = vec![
                v("enum", H::Strs),
                v("default", H::Str),
                v("description", H::Str),
                v("examples", H::Strs),
            ];
            if kind == "parameter" {
                table.push(v("location", H::Str));
            }
            table
        }
        "channel" => vec![
            v("address", H::StrNull),
            o("messages", H::MapRef("message"), "messages"),
            v("title", H::Str),
            v("summary", H::Str),
            v("description", H::Str),
            o("servers", H::Refs("server"), "servers"),
            o("parameters", H::MapRef("parameter"), "parameters"),
            tags,
            docs,
            o("bindings", H::OneRef("channel-bindings"), "bindings"),
        ],
        "operation" | "operation-trait" => {
            let mut table = Vec::new();
            if kind == "operation" {
                table.push(("action", H::Str, true, ""));
                table.push(("channel", H::Ref("channel"), true, "channel"));
            }
            table.extend([
                v("title", H::Str),
                v("summary", H::Str),
                v("description", H::Str),
                o("security", H::ListRef("security-scheme"), "security"),
                tags,
                docs,
                o("bindings", H::OneRef("operation-bindings"), "bindings"),
            ]);
            if kind == "operation" {
                table.extend([
                    o("traits", H::ListRef("operation-trait"), "traits"),
                    o("messages", H::Refs("message"), "messages"),
                    o("reply", H::OneRef("operation-reply"), "reply"),
                ]);
            }
            table
        }
        "operation-reply" => vec![
            o("address", H::OneRef("operation-reply-address"), "address"),
            o("channel", H::Ref("channel"), "channel"),
            o("messages", H::Refs("message"), "messages"),
        ],
        "operation-reply-address" | "correlation-id" => {
            vec![v("description", H::Str), ("location", H::Str, true, "")]
        }
        _ => Vec::new(),
    }
}

fn message_fields(kind: &str) -> Vec<F> {
    match kind {
        "message" | "message-trait" => {
            let mut table = vec![o("headers", H::Schema, "headers")];
            if kind == "message" {
                table.push(o("payload", H::Schema, "payload"));
            }
            table.extend([
                o(
                    "correlationId",
                    H::OneRef("correlation-id"),
                    "correlation-id",
                ),
                v("contentType", H::Str),
                v("name", H::Str),
                v("title", H::Str),
                v("summary", H::Str),
                v("description", H::Str),
                o("tags", H::ListRef("tag"), "tags"),
                o("externalDocs", H::OneRef("external-docs"), "external-docs"),
                o("bindings", H::OneRef("message-bindings"), "bindings"),
                o("examples", H::List("message-example"), "examples"),
            ]);
            if kind == "message" {
                table.push(o("traits", H::ListRef("message-trait"), "traits"));
            }
            table
        }
        "message-example" => vec![
            v("headers", H::AnyMap),
            v("payload", H::Any),
            v("name", H::Str),
            v("summary", H::Str),
        ],
        "server-bindings" | "channel-bindings" | "message-bindings" => {
            PROTOCOLS.iter().map(|p| v(p, H::AnyMap)).collect()
        }
        "operation-bindings" => {
            let mut names = PROTOCOLS.to_vec();
            names.swap(16, 17);
            names.into_iter().map(|p| v(p, H::AnyMap)).collect()
        }
        _ => Vec::new(),
    }
}

fn components_fields() -> Vec<F> {
    vec![
        o("schemas", H::Schemas, "schemas"),
        o("servers", H::MapRef("server"), "servers"),
        o("channels", H::MapRef("channel"), "channels"),
        o("operations", H::MapRef("operation"), "operations"),
        o("messages", H::MapRef("message"), "messages"),
        o(
            "securitySchemes",
            H::MapRef("security-scheme"),
            "security-schemes",
        ),
        o(
            "serverVariables",
            H::MapRef("server-variable"),
            "server-variables",
        ),
        o("parameters", H::MapRef("parameter"), "parameters"),
        o(
            "correlationIds",
            H::MapRef("correlation-id"),
            "correlation-ids",
        ),
        o("replies", H::MapRef("operation-reply"), "replies"),
        o(
            "replyAddresses",
            H::MapRef("operation-reply-address"),
            "reply-addresses",
        ),
        o("externalDocs", H::MapRef("external-docs"), "external-docs"),
        o("tags", H::MapRef("tag"), "tags"),
        o(
            "operationTraits",
            H::MapRef("operation-trait"),
            "operation-traits",
        ),
        o(
            "messageTraits",
            H::MapRef("message-trait"),
            "message-traits",
        ),
        o(
            "serverBindings",
            H::MapRef("server-bindings"),
            "server-bindings",
        ),
        o(
            "channelBindings",
            H::MapRef("channel-bindings"),
            "channel-bindings",
        ),
        o(
            "operationBindings",
            H::MapRef("operation-bindings"),
            "operation-bindings",
        ),
        o(
            "messageBindings",
            H::MapRef("message-bindings"),
            "message-bindings",
        ),
    ]
}

fn other_fields(kind: &str) -> Vec<F> {
    match kind {
        "tag" => vec![
            ("name", H::Str, true, ""),
            v("description", H::Str),
            o("externalDocs", H::OneRef("external-docs"), "external-docs"),
        ],
        "external-docs" => vec![v("description", H::Str), ("url", H::Str, true, "")],
        "components" => components_fields(),
        "multi-format-schema" => vec![
            ("schemaFormat", H::Str, true, ""),
            ("schema", H::Format, true, "schema"),
        ],
        "security-scheme" => vec![
            ("type", H::Str, true, ""),
            v("description", H::Str),
            v("name", H::Str),
            v("in", H::Str),
            v("scheme", H::Str),
            v("bearerFormat", H::Str),
            o("flows", H::One("oauth-flows"), "flows"),
            v("openIdConnectUrl", H::Str),
            v("scopes", H::Strs),
        ],
        "oauth-flows" => vec![
            o("implicit", H::One("oauth-flow"), "implicit"),
            o("password", H::One("oauth-flow"), "password"),
            o(
                "clientCredentials",
                H::One("oauth-flow"),
                "client-credentials",
            ),
            o(
                "authorizationCode",
                H::One("oauth-flow"),
                "authorization-code",
            ),
        ],
        "oauth-flow" => vec![
            v("authorizationUrl", H::Str),
            v("tokenUrl", H::Str),
            v("refreshUrl", H::Str),
            ("availableScopes", H::StrMap, true, ""),
        ],
        _ => Vec::new(),
    }
}

/// Whether `version` is `3.0.` or `3.1.` and digits, with an optional `-`
/// and a non-empty alphanumeric suffix.
fn is_3(version: &str) -> bool {
    let Some(rest) = version
        .strip_prefix("3.0.")
        .or_else(|| version.strip_prefix("3.1."))
    else {
        return false;
    };
    let (patch, suffix) = rest
        .find('-')
        .map_or((rest, None), |i| (&rest[..i], Some(&rest[i + 1..])));
    !patch.is_empty()
        && patch.chars().all(|c| c.is_ascii_digit())
        && suffix.is_none_or(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Whether a key is a server or parameter name, `^[A-Za-z0-9_\-]+$`.
fn server_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Whether a key is a component name, `^[a-zA-Z0-9\.\-_]+$`.
fn component_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// Whether `text` is a type and subtype of RFC 6838 restricted names, with
/// anything after a `;`.
fn specific_media_type(text: &str) -> bool {
    let essence = text.find(';').map_or(text, |i| &text[..i]);
    let parts: Vec<&str> = essence.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|name| {
            (1..=127).contains(&name.len())
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric())
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "!#$&-^_.+".contains(c))
        })
}

/// Whether `text` is `$message.header` or `$message.payload`, then nothing
/// or `#` and a JSON pointer whose every `~` escapes `0` or `1`.
fn runtime(text: &str) -> bool {
    let rest = text
        .strip_prefix("$message.header")
        .or_else(|| text.strip_prefix("$message.payload"));
    match rest {
        Some("") => true,
        Some(r) => r.strip_prefix('#').is_some_and(|pointer| {
            let chars: Vec<char> = pointer.chars().collect();
            (pointer.is_empty() || pointer.starts_with('/'))
                && chars
                    .iter()
                    .enumerate()
                    .all(|(i, c)| *c != '~' || matches!(chars.get(i + 1), Some('0' | '1')))
        }),
        None => false,
    }
}

/// The expressions of an address, or `None` when braces do not pair
/// around a name.
fn address_names(address: &str) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut current: Option<String> = None;
    for c in address.chars() {
        match (c, current.as_mut()) {
            ('{', None) => current = Some(String::new()),
            ('}', Some(name)) if !name.is_empty() => {
                names.push(name.clone());
                current = None;
            }
            ('{' | '}', _) => return None,
            (_, Some(name)) => name.push(c),
            (_, None) => {}
        }
    }
    current.is_none().then_some(names)
}

/// Whether a schema format reads, and with the `AsyncAPI` vocabulary.
fn format(text: &str) -> Option<bool> {
    for media in [
        "application/vnd.aai.asyncapi",
        "application/vnd.aai.asyncapi+json",
        "application/vnd.aai.asyncapi+yaml",
    ] {
        if let Some(version) = text
            .strip_prefix(media)
            .and_then(|r| r.strip_prefix(";version="))
            && is_3(version)
        {
            return Some(true);
        }
    }
    matches!(
        text,
        "application/schema+json;version=draft-07" | "application/schema+yaml;version=draft-07"
    )
    .then_some(false)
}

/// The kinds a place of `found` may stand for where `expected` is wanted.
fn stands_for(expected: &str, found: &str) -> bool {
    found == expected || (expected == "schema" && found == "multi-format-schema")
}

type Place = (usize, String);

struct Rd<'d> {
    docs: &'d [Document],
    d: usize,
    out: Reading,
    /// Each object's place: its kind, whether a Reference Object, its node.
    slots: BTreeMap<Place, (&'static str, bool, &'d Doc)>,
    order: Vec<(&'static str, usize, String)>,
    pending: Vec<(usize, String, String, &'static str)>,
    schemas: Vec<js::Draft07Embedded<'d>>,
    schema_edges: Vec<(String, &'static str, Option<String>, String)>,
    ref_edges: Vec<(String, String, String)>,
    formats: BTreeMap<Place, bool>,
    targets: BTreeMap<Place, Place>,
}

/// Read the documents of a description, the first an `AsyncAPI` document,
/// as the `AsyncAPI` page states.
///
/// # Errors
///
/// The first [`Refusal`], in the page's order.
pub fn read(docs: &[Document]) -> Result<Reading, Refusal> {
    let mut uris: Vec<Option<String>> = Vec::new();
    for (d, (uri, _, _)) in docs.iter().enumerate() {
        let uri = uri
            .as_ref()
            .map(|u| u.strip_suffix('#').unwrap_or(u).to_owned());
        if let Some(u) = &uri
            && (u.contains('#') || resolve(None, u).is_none())
        {
            return refuse(d, "", "Invalid");
        }
        if uris.contains(&uri) {
            return refuse(d, "", "DuplicateDocument");
        }
        uris.push(uri);
    }
    if docs.first().is_some_and(|doc| doc.2) {
        return refuse(0, "", "Invalid");
    }
    let mut rd = Rd {
        docs,
        d: 0,
        out: Reading::default(),
        slots: BTreeMap::new(),
        order: Vec::new(),
        pending: Vec::new(),
        schemas: Vec::new(),
        schema_edges: Vec::new(),
        ref_edges: Vec::new(),
        formats: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    for (d, (_, doc, fragment)) in docs.iter().enumerate() {
        if !fragment {
            rd.d = d;
            rd.document(doc)?;
        }
    }
    rd.references(&uris)?;
    rd.schema_objects(&uris)?;
    rd.joins()?;
    Ok(rd.out)
}

impl<'d> Rd<'d> {
    fn prefix(&self, d: usize) -> String {
        self.docs[d]
            .0
            .as_ref()
            .map(|u| u.strip_suffix('#').unwrap_or(u).to_owned())
            .unwrap_or_default()
    }

    fn id(&self, path: &str) -> String {
        format!("{}#{path}", self.prefix(self.d))
    }

    fn edge(&mut self, src: &str, kind: &str, tgt: &str, name: Option<String>) {
        self.out.edges.entry(src.to_owned()).or_default().push((
            kind.to_owned(),
            tgt.to_owned(),
            name,
        ));
    }

    fn keep(&mut self, id: &str, sort: &str, value: String) {
        self.out
            .constraints
            .entry(id.to_owned())
            .or_default()
            .push((sort.to_owned(), value));
    }

    fn document(&mut self, root: &'d Doc) -> R<()> {
        let d = self.d;
        if !matches!(root, Doc::Map(_)) {
            return refuse(d, "", "WrongType");
        }
        let Some(version) = get(root, "asyncapi") else {
            return refuse(d, "", "NotAsyncApi");
        };
        let Doc::Str(version) = version else {
            return refuse(d, "/asyncapi", "WrongType");
        };
        if !is_3(version) {
            return refuse(d, "/asyncapi", "Version");
        }
        let id = self.object("asyncapi", root, "")?;
        if !self.out.entries.contains(&id) {
            self.out.entries.push(id.clone());
        }
        self.out.roots.push(id);
        Ok(())
    }

    /// The id of a place read before, if it was: refused when it was read
    /// as another kind.
    fn before(&self, kind: &str, path: &str) -> Option<R<String>> {
        let (found, _, _) = self.slots.get(&(self.d, path.to_owned()))?;
        Some(if stands_for(kind, found) {
            Ok(self.id(path))
        } else {
            refuse(self.d, path, "Invalid")
        })
    }

    fn object(&mut self, kind: &'static str, node: &'d Doc, path: &str) -> R<String> {
        if let Some(read) = self.before(kind, path) {
            return read;
        }
        let d = self.d;
        let Doc::Map(members) = node else {
            return refuse(d, path, "WrongType");
        };
        let table = fields(kind);
        for (key, _) in members {
            if key.starts_with("x-") {
                if !extension(key) {
                    return refuse(d, &at(path, key), "Name");
                }
            } else if !table.iter().any(|f| f.0 == key) {
                return refuse(d, &at(path, key), "UnknownField");
            }
        }
        for f in &table {
            if f.2 && get(node, f.0).is_none() {
                return refuse(d, path, "Missing");
            }
        }
        self.rules(kind, node, path)?;
        let id = self.id(path);
        self.out.vertices.insert(id.clone(), kind.to_owned());
        self.slots.insert((d, path.to_owned()), (kind, false, node));
        self.order.push((kind, d, path.to_owned()));
        for f in &table {
            if let Some(value) = get(node, f.0) {
                self.field(&id, kind, f, value, &at(path, f.0))?;
            }
        }
        for (key, value) in members {
            if key.starts_with("x-") {
                self.keep(
                    &id,
                    "extension",
                    format!("{{{}:{}}}", json_string(key), json(value)),
                );
            }
        }
        Ok(id)
    }

    fn reference(&mut self, kind: &'static str, node: &'d Doc, path: &str) -> R<String> {
        if let Some(read) = self.before(kind, path) {
            return read;
        }
        let d = self.d;
        let Some(Doc::Str(text)) = get(node, "$ref") else {
            return refuse(d, &at(path, "$ref"), "WrongType");
        };
        self.pending.push((d, path.to_owned(), text.clone(), kind));
        let id = self.id(path);
        self.out.vertices.insert(id.clone(), "reference".to_owned());
        self.slots.insert((d, path.to_owned()), (kind, true, node));
        if let Doc::Map(members) = node {
            for (key, value) in members.iter().filter(|(k, _)| k != "$ref") {
                self.keep(
                    &id,
                    "ignored",
                    format!("{{{}:{}}}", json_string(key), json(value)),
                );
            }
        }
        Ok(id)
    }

    fn object_or_ref(&mut self, kind: &'static str, node: &'d Doc, path: &str) -> R<String> {
        if get(node, "$ref").is_some() {
            self.reference(kind, node, path)
        } else {
            self.object(kind, node, path)
        }
    }

    fn only_ref(&mut self, kind: &'static str, node: &'d Doc, path: &str) -> R<String> {
        match node {
            Doc::Map(_) if get(node, "$ref").is_some() => self.reference(kind, node, path),
            Doc::Map(_) => refuse(self.d, path, "Invalid"),
            _ => refuse(self.d, path, "WrongType"),
        }
    }

    /// A schema place: a Reference Object, a Multi Format Schema Object, or
    /// a Schema Object read later.
    fn schema_place(&mut self, node: &'d Doc, path: &str) -> R<()> {
        match node {
            Doc::Map(_) if get(node, "$ref").is_some() => {
                self.reference("schema", node, path).map(|_| ())
            }
            Doc::Map(_) if get(node, "schemaFormat").is_some() => {
                self.object("multi-format-schema", node, path).map(|_| ())
            }
            Doc::Map(_) | Doc::Bool(_) => self.embed(node, path, true),
            _ => refuse(self.d, path, "WrongType"),
        }
    }

    fn embed(&mut self, node: &'d Doc, path: &str, asyncapi: bool) -> R<()> {
        if let Some(read) = self.before("schema", path) {
            return read.map(|_| ());
        }
        self.slots
            .insert((self.d, path.to_owned()), ("schema", false, node));
        self.schemas.push((self.d, path.to_owned(), node, asyncapi));
        Ok(())
    }

    fn schema_field(
        &mut self,
        id: &str,
        edge: &'static str,
        name: Option<String>,
        node: &'d Doc,
        path: &str,
    ) -> R<()> {
        self.schema_place(node, path)?;
        let target = self.id(path);
        self.schema_edges.push((id.to_owned(), edge, name, target));
        Ok(())
    }

    fn field(&mut self, id: &str, kind: &str, spec: &F, value: &'d Doc, path: &str) -> R<()> {
        let d = self.d;
        let (name, holds, _, edge) = *spec;
        let fits = match holds {
            H::Str => matches!(value, Doc::Str(_)),
            H::StrNull => matches!(value, Doc::Str(_) | Doc::Null),
            H::Strs => matches!(value, Doc::Seq(i) if i.iter().all(|x| matches!(x, Doc::Str(_)))),
            H::StrMap => {
                matches!(value, Doc::Map(m) if m.iter().all(|(_, x)| matches!(x, Doc::Str(_))))
            }
            H::AnyMap => matches!(value, Doc::Map(_)),
            _ => true,
        };
        if !fits {
            return refuse(d, path, "WrongType");
        }
        match holds {
            H::Str | H::StrNull | H::Any | H::Strs | H::StrMap | H::AnyMap => {
                self.keep(id, name, json(value));
            }
            H::One(target) => {
                let target = self.object(target, value, path)?;
                self.edge(id, edge, &target, None);
            }
            H::OneRef(target) => {
                let target = self.object_or_ref(target, value, path)?;
                self.edge(id, edge, &target, None);
            }
            H::Ref(target) => {
                let target = self.only_ref(target, value, path)?;
                self.edge(id, edge, &target, None);
            }
            H::Schema => self.schema_field(id, edge, None, value, path)?,
            H::Format => {
                if !matches!(value, Doc::Map(_) | Doc::Bool(_)) {
                    return refuse(d, path, "WrongType");
                }
                let parent = path.rfind('/').map_or("", |i| &path[..i]).to_owned();
                let asyncapi = self.formats.get(&(d, parent)).copied().unwrap_or(false);
                self.embed(value, path, asyncapi)?;
                let target = self.id(path);
                self.schema_edges.push((id.to_owned(), edge, None, target));
            }
            H::Refs(target) | H::List(target) | H::ListRef(target) => {
                let Doc::Seq(items) = value else {
                    return refuse(d, path, "WrongType");
                };
                if items.is_empty() {
                    self.keep(id, name, "[]".to_owned());
                }
                for (i, item) in items.iter().enumerate() {
                    let place = format!("{path}/{i}");
                    let target = match holds {
                        H::Refs(_) => self.only_ref(target, item, &place)?,
                        H::ListRef(_) => self.object_or_ref(target, item, &place)?,
                        _ => self.object(target, item, &place)?,
                    };
                    self.edge(id, edge, &target, Some(i.to_string()));
                }
            }
            H::MapRef(_) | H::Schemas => {
                let Doc::Map(entries) = value else {
                    return refuse(d, path, "WrongType");
                };
                if entries.is_empty() {
                    self.keep(id, name, "{}".to_owned());
                }
                let rule: Option<fn(&str) -> bool> = match (kind, name) {
                    ("components", _) => Some(component_key),
                    ("asyncapi", "servers") | ("channel", "parameters") => Some(server_key),
                    _ => None,
                };
                for (key, entry) in entries {
                    let entry_path = at(path, key);
                    if rule.is_some_and(|ok| !ok(key)) {
                        return refuse(d, &entry_path, "Name");
                    }
                    if let H::MapRef(target) = holds {
                        let found = self.object_or_ref(target, entry, &entry_path)?;
                        self.edge(id, edge, &found, Some(key.clone()));
                    } else {
                        self.schema_field(id, edge, Some(key.clone()), entry, &entry_path)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'d> Rd<'d> {
    /// The rules of one object its field table does not state.
    fn rules(&mut self, kind: &str, node: &Doc, path: &str) -> R<()> {
        let d = self.d;
        let urls: &[&str] = match kind {
            "asyncapi" => &["id"],
            "info" => &["termsOfService"],
            "contact" | "license" | "external-docs" => &["url"],
            "security-scheme" => &["openIdConnectUrl"],
            "oauth-flow" => &["authorizationUrl", "tokenUrl", "refreshUrl"],
            _ => &[],
        };
        for name in urls {
            if let Some(Doc::Str(t)) = get(node, name)
                && !has_scheme(t)
            {
                return refuse(d, &at(path, name), "Invalid");
            }
        }
        let media = match kind {
            "asyncapi" => "defaultContentType",
            "message" | "message-trait" => "contentType",
            _ => "",
        };
        if let Some(Doc::Str(t)) = get(node, media)
            && !specific_media_type(t)
        {
            return refuse(d, &at(path, media), "Invalid");
        }
        match kind {
            "channel" => self.channel_rules(node, path),
            "operation" => match get(node, "action") {
                Some(Doc::Str(a)) if a != "send" && a != "receive" => {
                    refuse(d, &at(path, "action"), "Invalid")
                }
                _ => Ok(()),
            },
            "operation-reply-address" | "correlation-id" | "parameter" => {
                match get(node, "location") {
                    Some(Doc::Str(l)) if !runtime(l) => refuse(d, &at(path, "location"), "Invalid"),
                    _ => Ok(()),
                }
            }
            "message-example"
                if get(node, "headers").is_none() && get(node, "payload").is_none() =>
            {
                refuse(d, path, "Missing")
            }
            "multi-format-schema" => {
                if let Some(Doc::Str(t)) = get(node, "schemaFormat") {
                    let Some(asyncapi) = format(t) else {
                        return refuse(d, &at(path, "schemaFormat"), "SchemaFormat");
                    };
                    self.formats.insert((d, path.to_owned()), asyncapi);
                }
                Ok(())
            }
            "security-scheme" => self.scheme_rules(node, path),
            "oauth-flows" => self.flow_rules(node, path),
            _ => Ok(()),
        }
    }

    fn channel_rules(&self, node: &Doc, path: &str) -> R<()> {
        let d = self.d;
        let names = match get(node, "address") {
            Some(Doc::Str(address)) => {
                if address.contains('?') || address.contains('#') {
                    return refuse(d, &at(path, "address"), "Invalid");
                }
                let Some(names) = address_names(address) else {
                    return refuse(d, &at(path, "address"), "Template");
                };
                names
            }
            _ => Vec::new(),
        };
        let Some(parameters) = get(node, "parameters") else {
            return Ok(());
        };
        let here = at(path, "parameters");
        if names.is_empty() {
            return refuse(d, &here, "Invalid");
        }
        let Doc::Map(entries) = parameters else {
            return Ok(());
        };
        if names.iter().any(|n| !entries.iter().any(|(k, _)| k == n)) {
            return refuse(d, &here, "MissingParameter");
        }
        if let Some((key, _)) = entries.iter().find(|(k, _)| !names.contains(k)) {
            return refuse(d, &at(&here, key), "UnknownParameter");
        }
        Ok(())
    }

    fn scheme_rules(&self, node: &Doc, path: &str) -> R<()> {
        let d = self.d;
        let Some(Doc::Str(kind)) = get(node, "type") else {
            return Ok(());
        };
        let (applies, needs): (&[&str], &[&str]) = match kind.as_str() {
            "userPassword"
            | "X509"
            | "symmetricEncryption"
            | "asymmetricEncryption"
            | "plain"
            | "scramSha256"
            | "scramSha512"
            | "gssapi" => (&[], &[]),
            "apiKey" => (&["in"], &["in"]),
            "httpApiKey" => (&["name", "in"], &["name", "in"]),
            "http" => (&["scheme", "bearerFormat"], &["scheme"]),
            "oauth2" => (&["flows", "scopes"], &["flows"]),
            "openIdConnect" => (&["openIdConnectUrl", "scopes"], &["openIdConnectUrl"]),
            _ => return refuse(d, &at(path, "type"), "Invalid"),
        };
        for name in [
            "name",
            "in",
            "scheme",
            "bearerFormat",
            "flows",
            "openIdConnectUrl",
            "scopes",
        ] {
            if get(node, name).is_some() && !applies.contains(&name) {
                return refuse(d, &at(path, name), "Invalid");
            }
        }
        if needs.iter().any(|n| get(node, n).is_none()) {
            return refuse(d, path, "Missing");
        }
        if let Some(Doc::Str(location)) = get(node, "in") {
            let allowed: &[&str] = if kind == "apiKey" {
                &["user", "password"]
            } else {
                &["query", "header", "cookie"]
            };
            if !allowed.contains(&location.as_str()) {
                return refuse(d, &at(path, "in"), "Invalid");
            }
        }
        let bearer =
            matches!(get(node, "scheme"), Some(Doc::Str(s)) if s.eq_ignore_ascii_case("bearer"));
        if get(node, "bearerFormat").is_some() && !bearer {
            return refuse(d, &at(path, "bearerFormat"), "Invalid");
        }
        Ok(())
    }

    fn flow_rules(&self, node: &Doc, path: &str) -> R<()> {
        let d = self.d;
        for (flow, needs, never) in [
            ("implicit", &["authorizationUrl"][..], Some("tokenUrl")),
            ("password", &["tokenUrl"][..], Some("authorizationUrl")),
            (
                "clientCredentials",
                &["tokenUrl"][..],
                Some("authorizationUrl"),
            ),
            (
                "authorizationCode",
                &["authorizationUrl", "tokenUrl"][..],
                None,
            ),
        ] {
            let Some(value @ Doc::Map(_)) = get(node, flow) else {
                continue;
            };
            let here = at(path, flow);
            if needs.iter().any(|n| get(value, n).is_none()) {
                return refuse(d, &here, "Missing");
            }
            if let Some(never) = never
                && get(value, never).is_some()
            {
                return refuse(d, &at(&here, never), "Invalid");
            }
        }
        Ok(())
    }

    /// The place a reference made in document `d` names.
    fn locate(uris: &[Option<String>], d: usize, reference: &str) -> Result<Place, &'static str> {
        let (doc, fragment) = if let Some(f) = reference.strip_prefix('#') {
            (d, f.to_owned())
        } else {
            let full = resolve(uris[d].as_deref(), reference).ok_or("External")?;
            let (uri, f) = full.split_once('#').unwrap_or((&full, ""));
            let doc = uris
                .iter()
                .position(|u| u.as_deref() == Some(uri))
                .ok_or("External")?;
            (doc, f.to_owned())
        };
        if !fragment.is_empty() && !fragment.starts_with('/') {
            return Err("RefTarget");
        }
        let mut bytes = Vec::new();
        let mut rest = fragment.as_bytes();
        while let Some((&first, tail)) = rest.split_first() {
            if first == b'%' {
                let hex = tail
                    .get(..2)
                    .and_then(|h| std::str::from_utf8(h).ok())
                    .ok_or("RefTarget")?;
                bytes.push(u8::from_str_radix(hex, 16).map_err(|_| "RefTarget")?);
                rest = &tail[2..];
            } else {
                bytes.push(first);
                rest = tail;
            }
        }
        Ok((doc, String::from_utf8(bytes).map_err(|_| "RefTarget")?))
    }

    /// The node at a JSON pointer of a fragment document.
    fn node_at(&self, place: &Place) -> Option<&'d Doc> {
        let (_, tree, fragment) = &self.docs[place.0];
        if !fragment {
            return None;
        }
        let mut node = tree;
        if place.1.is_empty() {
            return Some(node);
        }
        for token in place.1.strip_prefix('/')?.split('/') {
            let key = token.replace("~1", "/").replace("~0", "~");
            node = match node {
                Doc::Map(members) => &members.iter().find(|(k, _)| *k == key)?.1,
                Doc::Seq(items) => {
                    let ok = !key.is_empty()
                        && key.chars().all(|c| c.is_ascii_digit())
                        && (key == "0" || !key.starts_with('0'));
                    if !ok {
                        return None;
                    }
                    items.get(key.parse::<usize>().ok()?)?
                }
                _ => return None,
            };
        }
        Some(node)
    }

    fn references(&mut self, uris: &[Option<String>]) -> R<()> {
        let mut next = 0;
        while let Some((d, path, reference, expected)) = self.pending.get(next).cloned() {
            next += 1;
            let key = at(&path, "$ref");
            let place =
                Self::locate(uris, d, &reference).or_else(|class| refuse(d, &key, class))?;
            if !self.slots.contains_key(&place)
                && let Some(node) = self.node_at(&place)
            {
                self.d = place.0;
                if expected == "schema" {
                    self.schema_place(node, &place.1)?;
                } else {
                    self.object_or_ref(expected, node, &place.1)?;
                }
            }
            match self.slots.get(&place) {
                None => return refuse(d, &key, "RefTarget"),
                Some((kind, _, _)) if !stands_for(expected, kind) => {
                    return refuse(d, &key, "RefKind");
                }
                Some(("multi-format-schema", _, _)) if self.formats.get(&place) == Some(&false) => {
                    return refuse(d, &key, "FormatMismatch");
                }
                Some(_) => {}
            }
            let src = format!("{}#{path}", self.prefix(d));
            let tgt = format!("{}#{}", self.prefix(place.0), place.1);
            self.ref_edges.push((src, tgt, reference.clone()));
            self.targets.insert((d, path.clone()), place);
        }
        for (d, path, _, _) in &self.pending {
            let start = (*d, path.clone());
            let mut visited = BTreeSet::from([start.clone()]);
            let mut place = start;
            while let Some(next) = self.targets.get(&place) {
                if !visited.insert(next.clone()) {
                    return refuse(*d, &at(path, "$ref"), "RefCycle");
                }
                place = next.clone();
            }
        }
        Ok(())
    }

    fn schema_objects(&mut self, uris: &[Option<String>]) -> R<()> {
        let fragments: Vec<Option<&Doc>> = self
            .docs
            .iter()
            .map(|(_, tree, fragment)| fragment.then_some(tree))
            .collect();
        let read = js::read_draft07(uris, &fragments, &self.schemas).map_err(|r| Refusal {
            document: r.document,
            path: r.path,
            class: "Schema",
        })?;
        self.out.vertices.extend(read.vertices);
        for (id, edges) in read.edges {
            self.out.edges.entry(id).or_default().extend(edges);
        }
        self.out.constraints.extend(read.constraints);
        self.out.required.extend(read.required);
        for entry in read.entries {
            if !self.out.entries.contains(&entry) {
                self.out.entries.push(entry);
            }
        }
        for (src, edge, name, tgt) in std::mem::take(&mut self.schema_edges) {
            self.edge(&src, edge, &tgt, name);
        }
        for (src, tgt, name) in std::mem::take(&mut self.ref_edges) {
            self.edge(&src, "ref", &tgt, Some(name));
        }
        Ok(())
    }

    /// Whether `place` is `/{root}/{key}{suffix}` of an `AsyncAPI` document.
    fn rooted(&self, place: &Place, root: &str, suffix: &str) -> bool {
        let head = format!("/{root}/");
        !self.docs[place.0].2
            && place.1.starts_with(&head)
            && place.1.ends_with(suffix)
            && place.1.len() > head.len() + suffix.len()
            && !place.1[head.len()..place.1.len() - suffix.len()].contains('/')
    }

    fn placement(item: &Place) -> R<()> {
        refuse(item.0, &at(&item.1, "$ref"), "Placement")
    }

    fn joins(&self) -> R<()> {
        for (kind, d, path) in &self.order {
            let here = (*d, path.clone());
            let node = self.slots[&here].2;
            let (channel_root, reply) = match *kind {
                "channel" => {
                    if self.rooted(&here, "channels", "")
                        && let Some(Doc::Seq(items)) = get(node, "servers")
                    {
                        for i in 0..items.len() {
                            let item = (*d, format!("{path}/servers/{i}"));
                            if let Some(t) = self.targets.get(&item)
                                && !(t.0 == *d && self.rooted(t, "servers", ""))
                            {
                                return Self::placement(&item);
                            }
                        }
                    }
                    continue;
                }
                "operation" => (self.rooted(&here, "operations", ""), false),
                "operation-reply" => (self.rooted(&here, "operations", "/reply"), true),
                _ => continue,
            };
            let channel = (*d, at(path, "channel"));
            if !self.slots.contains_key(&channel) {
                continue;
            }
            if channel_root
                && let Some(t) = self.targets.get(&channel)
                && !(t.0 == *d && self.rooted(t, "channels", ""))
            {
                return Self::placement(&channel);
            }
            let mut target = channel;
            while let Some((_, true, _)) = self.slots.get(&target) {
                match self.targets.get(&target) {
                    Some(t) => target = t.clone(),
                    None => break,
                }
            }
            let Some((_, _, channel_node)) = self.slots.get(&target) else {
                continue;
            };
            if reply
                && get(node, "address").is_some()
                && !matches!(get(channel_node, "address"), None | Some(Doc::Null))
            {
                return refuse(*d, &at(path, "address"), "Invalid");
            }
            let Some(Doc::Seq(items)) = get(node, "messages") else {
                continue;
            };
            let head = format!("{}/messages/", target.1);
            for i in 0..items.len() {
                let item = (*d, format!("{path}/messages/{i}"));
                let Some(named) = self.targets.get(&item) else {
                    continue;
                };
                let key = named.1.strip_prefix(&head);
                if named.0 != target.0 || !key.is_some_and(|k| !k.is_empty() && !k.contains('/')) {
                    return refuse(item.0, &at(&item.1, "$ref"), "NotInChannel");
                }
            }
        }
        Ok(())
    }
}

/// The canonical text of an `AsyncAPI` document, followed by a line feed.
///
/// It holds each object's fields in the order of the specification's
/// tables and its extensions last; a Reference Object's `$ref` and then its
/// other members; a Schema Object as the JSON Schema reference writes it.
#[must_use]
pub fn canonical_text(doc: &Doc) -> String {
    format!("{}\n", canonical("asyncapi", doc))
}

fn canonical_ref(node: &Doc) -> String {
    let Doc::Map(members) = node else {
        return json(node);
    };
    let parts: Vec<String> = members
        .iter()
        .filter(|(k, _)| k == "$ref")
        .chain(members.iter().filter(|(k, _)| k != "$ref"))
        .map(|(k, v)| format!("{}:{}", json_string(k), json(v)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

fn canonical_or_ref(kind: &str, node: &Doc) -> String {
    if get(node, "$ref").is_some() {
        canonical_ref(node)
    } else {
        canonical(kind, node)
    }
}

fn canonical_json_schema(node: &Doc) -> String {
    js::canonical_text(node).trim_end_matches('\n').to_owned()
}

fn canonical_schema(node: &Doc) -> String {
    if get(node, "$ref").is_some() {
        canonical_ref(node)
    } else if get(node, "schemaFormat").is_some() {
        canonical("multi-format-schema", node)
    } else {
        canonical_json_schema(node)
    }
}

fn canonical(kind: &str, node: &Doc) -> String {
    let Doc::Map(members) = node else {
        return json(node);
    };
    let mut parts = Vec::new();
    for (name, h, _, _) in fields(kind) {
        let Some(value) = get(node, name) else {
            continue;
        };
        let list = |write: &dyn Fn(&Doc) -> String| match value {
            Doc::Seq(items) if !items.is_empty() => format!(
                "[{}]",
                items.iter().map(write).collect::<Vec<_>>().join(",")
            ),
            other => json(other),
        };
        let map = |write: &dyn Fn(&Doc) -> String| match value {
            Doc::Map(entries) if !entries.is_empty() => format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(k, v)| format!("{}:{}", json_string(k), write(v)))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            other => json(other),
        };
        let written = match h {
            H::One(k) => canonical(k, value),
            H::OneRef(k) => canonical_or_ref(k, value),
            H::Ref(_) => canonical_ref(value),
            H::Schema => canonical_schema(value),
            H::Format => canonical_json_schema(value),
            H::Refs(_) => list(&canonical_ref),
            H::List(k) => list(&|i| canonical(k, i)),
            H::ListRef(k) => list(&|i| canonical_or_ref(k, i)),
            H::MapRef(k) => map(&|e| canonical_or_ref(k, e)),
            H::Schemas => map(&canonical_schema),
            _ => json(value),
        };
        parts.push(format!("{}:{written}", json_string(name)));
    }
    for (k, value) in members.iter().filter(|(k, _)| k.starts_with("x-")) {
        parts.push(format!("{}:{}", json_string(k), json(value)));
    }
    format!("{{{}}}", parts.join(","))
}
