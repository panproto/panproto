//! The `OpenAPI` page's reference: an `OpenAPI` 3.1 document read as the
//! page states, over data trees, with its own field table, and its
//! canonical text.
//!
//! Schema Objects are read by the JSON Schema reference
//! ([`super::json_schema::read_embedded`]) under the OAS dialect.

use std::collections::{BTreeMap, BTreeSet};

use super::json_schema::{self as js, Doc, Reading, json, json_string, resolve, token};

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

fn text(node: Option<&Doc>) -> Option<&str> {
    match node {
        Some(Doc::Str(t)) => Some(t),
        _ => None,
    }
}

/// How a field holds its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum H {
    Str,
    Bool,
    Any,
    Strs,
    StrMap,
    AnyMap,
    One(&'static str),
    OneRef(&'static str),
    List(&'static str),
    ListRef(&'static str),
    Map(&'static str),
    MapRef(&'static str),
    Schema,
    Schemas,
    Pat(&'static str, bool),
}

/// A field: name, how it holds, whether REQUIRED, and its edge kind.
type F = (&'static str, H, bool, &'static str);

const fn v(name: &'static str, h: H) -> F {
    (name, h, false, "")
}

const fn o(name: &'static str, h: H, edge: &'static str) -> F {
    (name, h, false, edge)
}

const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// The fields of each object kind, in the specification's order.
fn fields(kind: &str) -> Vec<F> {
    [description_fields, operation_fields, security_fields]
        .iter()
        .map(|table| table(kind))
        .find(|found| !found.is_empty())
        .unwrap_or_default()
}

/// The fields of the objects that describe the API as a whole.
fn description_fields(kind: &str) -> Vec<F> {
    match kind {
        "openapi" => vec![
            ("openapi", H::Str, true, ""),
            ("info", H::One("info"), true, "info"),
            v("jsonSchemaDialect", H::Str),
            o("servers", H::List("server"), "servers"),
            o("paths", H::One("paths"), "paths"),
            o("webhooks", H::Map("path-item"), "webhooks"),
            o("components", H::One("components"), "components"),
            o("security", H::List("security-requirement"), "security"),
            o("tags", H::List("tag"), "tags"),
            o("externalDocs", H::One("external-docs"), "external-docs"),
        ],
        "info" => vec![
            ("title", H::Str, true, ""),
            v("summary", H::Str),
            v("description", H::Str),
            v("termsOfService", H::Str),
            o("contact", H::One("contact"), "contact"),
            o("license", H::One("license"), "license"),
            ("version", H::Str, true, ""),
        ],
        "contact" => vec![v("name", H::Str), v("url", H::Str), v("email", H::Str)],
        "license" => vec![
            ("name", H::Str, true, ""),
            v("identifier", H::Str),
            v("url", H::Str),
        ],
        "server" => vec![
            ("url", H::Str, true, ""),
            v("description", H::Str),
            o("variables", H::Map("server-variable"), "variables"),
        ],
        "server-variable" => vec![
            v("enum", H::Strs),
            ("default", H::Str, true, ""),
            v("description", H::Str),
        ],
        "components" => vec![
            o("schemas", H::Schemas, "schemas"),
            o("responses", H::MapRef("response"), "responses"),
            o("parameters", H::MapRef("parameter"), "parameters"),
            o("examples", H::MapRef("example"), "examples"),
            o("requestBodies", H::MapRef("request-body"), "request-bodies"),
            o("headers", H::MapRef("header"), "headers"),
            o(
                "securitySchemes",
                H::MapRef("security-scheme"),
                "security-schemes",
            ),
            o("links", H::MapRef("link"), "links"),
            o("callbacks", H::MapRef("callback"), "callbacks"),
            o("pathItems", H::Map("path-item"), "path-items"),
        ],
        "paths" => vec![("", H::Pat("path-item", false), false, "path")],
        "path-item" => vec![
            v("summary", H::Str),
            v("description", H::Str),
            o("get", H::One("operation"), "get"),
            o("put", H::One("operation"), "put"),
            o("post", H::One("operation"), "post"),
            o("delete", H::One("operation"), "delete"),
            o("options", H::One("operation"), "options"),
            o("head", H::One("operation"), "head"),
            o("patch", H::One("operation"), "patch"),
            o("trace", H::One("operation"), "trace"),
            o("servers", H::List("server"), "servers"),
            o("parameters", H::ListRef("parameter"), "parameters"),
        ],
        "operation" => vec![
            v("tags", H::Strs),
            v("summary", H::Str),
            v("description", H::Str),
            o("externalDocs", H::One("external-docs"), "external-docs"),
            v("operationId", H::Str),
            o("parameters", H::ListRef("parameter"), "parameters"),
            o("requestBody", H::OneRef("request-body"), "request-body"),
            o("responses", H::One("responses"), "responses"),
            o("callbacks", H::MapRef("callback"), "callbacks"),
            v("deprecated", H::Bool),
            o("security", H::List("security-requirement"), "security"),
            o("servers", H::List("server"), "servers"),
        ],
        _ => Vec::new(),
    }
}

/// The fields of tags, security schemes and their flows.
fn security_fields(kind: &str) -> Vec<F> {
    match kind {
        "tag" => vec![
            ("name", H::Str, true, ""),
            v("description", H::Str),
            o("externalDocs", H::One("external-docs"), "external-docs"),
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
            ("scopes", H::StrMap, true, ""),
        ],
        _ => Vec::new(),
    }
}

/// The fields of the objects that describe operations and their parts.
fn operation_fields(kind: &str) -> Vec<F> {
    match kind {
        "external-docs" => vec![v("description", H::Str), ("url", H::Str, true, "")],
        "parameter" => vec![
            ("name", H::Str, true, ""),
            ("in", H::Str, true, ""),
            v("description", H::Str),
            v("required", H::Bool),
            v("deprecated", H::Bool),
            v("allowEmptyValue", H::Bool),
            v("style", H::Str),
            v("explode", H::Bool),
            v("allowReserved", H::Bool),
            o("schema", H::Schema, "schema"),
            v("example", H::Any),
            o("examples", H::MapRef("example"), "examples"),
            o("content", H::Map("media-type"), "content"),
        ],
        "request-body" => vec![
            v("description", H::Str),
            ("content", H::Map("media-type"), true, "content"),
            v("required", H::Bool),
        ],
        "media-type" => vec![
            o("schema", H::Schema, "schema"),
            v("example", H::Any),
            o("examples", H::MapRef("example"), "examples"),
            o("encoding", H::Map("encoding"), "encoding"),
        ],
        "encoding" => vec![
            v("contentType", H::Str),
            o("headers", H::MapRef("header"), "headers"),
            v("style", H::Str),
            v("explode", H::Bool),
            v("allowReserved", H::Bool),
        ],
        "responses" => vec![("", H::Pat("response", true), false, "response")],
        "response" => vec![
            ("description", H::Str, true, ""),
            o("headers", H::MapRef("header"), "headers"),
            o("content", H::Map("media-type"), "content"),
            o("links", H::MapRef("link"), "links"),
        ],
        "callback" => vec![("", H::Pat("path-item", false), false, "expression")],
        "example" => vec![
            v("summary", H::Str),
            v("description", H::Str),
            v("value", H::Any),
            v("externalValue", H::Str),
        ],
        "link" => vec![
            v("operationRef", H::Str),
            v("operationId", H::Str),
            v("parameters", H::AnyMap),
            v("requestBody", H::Any),
            v("description", H::Str),
            o("server", H::One("server"), "server"),
        ],
        "header" => vec![
            v("description", H::Str),
            v("required", H::Bool),
            v("deprecated", H::Bool),
            v("style", H::Str),
            v("explode", H::Bool),
            o("schema", H::Schema, "schema"),
            v("example", H::Any),
            o("examples", H::MapRef("example"), "examples"),
            o("content", H::Map("media-type"), "content"),
        ],
        _ => vec![],
    }
}

const REFERENCE_FIELDS: [F; 2] = [
    ("summary", H::Str, false, ""),
    ("description", H::Str, false, ""),
];

fn styles(location: &str) -> &'static [&'static str] {
    match location {
        "path" => &["matrix", "label", "simple"],
        "query" => &["form", "spaceDelimited", "pipeDelimited", "deepObject"],
        "header" => &["simple"],
        "cookie" => &["form"],
        _ => &[],
    }
}

/// Whether `version` is `3.1.<digits>` with an optional non-empty `-`
/// suffix.
fn is_3_1(version: &str) -> bool {
    let Some(rest) = version.strip_prefix("3.1.") else {
        return false;
    };
    let (patch, suffix) = rest
        .find('-')
        .map_or((rest, None), |i| (&rest[..i], Some(&rest[i + 1..])));
    !patch.is_empty() && patch.chars().all(|c| c.is_ascii_digit()) && suffix != Some("")
}

fn status_key(key: &str) -> bool {
    let c: Vec<char> = key.chars().collect();
    key == "default"
        || (c.len() == 3
            && ('1'..='5').contains(&c[0])
            && ((c[1].is_ascii_digit() && c[2].is_ascii_digit()) || (c[1] == 'X' && c[2] == 'X')))
}

/// The template expressions of a path, or `None` when braces do not pair.
fn templates(path: &str) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut current: Option<String> = None;
    for c in path.chars() {
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

fn shape(path: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in path.chars() {
        if c == '{' || c == '}' {
            inside = c == '{';
            out.push(c);
        } else if !inside {
            out.push(c);
        }
    }
    out
}

struct Rd<'d> {
    docs: &'d [(Option<String>, Doc)],
    d: usize,
    oas: Vec<bool>,
    out: Reading,
    /// Each object's place: its kind, whether a Reference Object, its node.
    slots: BTreeMap<(usize, String), (&'static str, bool, &'d Doc)>,
    order: Vec<(&'static str, usize, String)>,
    pending: Vec<(usize, String, String, &'static str)>,
    schemas: Vec<js::Embedded<'d>>,
    schema_edges: Vec<(String, &'static str, Option<String>, String)>,
    requirements: Vec<(usize, String, String)>,
    lists: Vec<(usize, String, Vec<String>)>,
    targets: BTreeMap<(usize, String), (usize, String)>,
}

/// Read the documents of a description, each an `OpenAPI` document with an
/// optional retrieval URI, as the `OpenAPI` page states.
///
/// # Errors
///
/// The first [`Refusal`], in the page's order.
pub fn read(docs: &[(Option<String>, Doc)]) -> Result<Reading, Refusal> {
    let mut seen: Vec<Option<String>> = Vec::new();
    for (d, (uri, _)) in docs.iter().enumerate() {
        let uri = uri
            .as_ref()
            .map(|u| u.strip_suffix('#').unwrap_or(u).to_owned());
        if let Some(u) = &uri
            && (u.contains('#') || resolve(None, u).is_none())
        {
            return refuse(d, "", "Invalid");
        }
        if seen.contains(&uri) {
            return refuse(d, "", "DuplicateDocument");
        }
        seen.push(uri);
    }
    let mut rd = Rd {
        docs,
        d: 0,
        oas: Vec::new(),
        out: Reading::default(),
        slots: BTreeMap::new(),
        order: Vec::new(),
        pending: Vec::new(),
        schemas: Vec::new(),
        schema_edges: Vec::new(),
        requirements: Vec::new(),
        lists: Vec::new(),
        targets: BTreeMap::new(),
    };
    for (d, (_, doc)) in docs.iter().enumerate() {
        rd.d = d;
        rd.document(doc)?;
    }
    rd.references(&seen)?;
    rd.schema_objects(&seen)?;
    let ids = rd.operation_ids()?;
    rd.links(&ids, &seen)?;
    rd.security()?;
    rd.parameter_lists()?;
    rd.paths()?;
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
        if get(root, "swagger").is_some() {
            return refuse(d, "/swagger", "Swagger");
        }
        let Some(version) = get(root, "openapi") else {
            return refuse(d, "", "NotOpenApi");
        };
        let Doc::Str(version) = version else {
            return refuse(d, "/openapi", "WrongType");
        };
        if !is_3_1(version) {
            return refuse(d, "/openapi", "Version");
        }
        let oas = match get(root, "jsonSchemaDialect") {
            None => true,
            Some(Doc::Str(t)) if t.strip_suffix('#').unwrap_or(t) == js::OAS_DIALECT => true,
            Some(Doc::Str(t)) if t.strip_suffix('#').unwrap_or(t) == js::DIALECT => false,
            Some(Doc::Str(_)) => return refuse(d, "/jsonSchemaDialect", "Dialect"),
            Some(_) => return refuse(d, "/jsonSchemaDialect", "WrongType"),
        };
        self.oas.push(oas);
        let id = self.object("openapi", root, "")?;
        if !self.out.entries.contains(&id) {
            self.out.entries.push(id.clone());
        }
        self.out.roots.push(id);
        Ok(())
    }

    fn object(&mut self, kind: &'static str, node: &'d Doc, path: &str) -> R<String> {
        let d = self.d;
        let Doc::Map(members) = node else {
            return refuse(d, path, "WrongType");
        };
        let table = fields(kind);
        let patterned = table.iter().any(|f| matches!(f.1, H::Pat(..)));
        for (key, _) in members {
            let known = key.starts_with("x-")
                || table.iter().any(|f| f.0 == key)
                || (kind == "path-item" && key == "$ref")
                || patterned
                || kind == "security-requirement";
            if !known {
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
        if kind == "path-item"
            && let Some(r) = get(node, "$ref")
        {
            self.reference(r, path, "path-item")?;
        }
        for f in &table {
            if let H::Pat(..) = f.1 {
                self.patterned(kind, &id, members, path, f)?;
            } else if let Some(value) = get(node, f.0) {
                self.field(&id, f, value, &at(path, f.0))?;
            }
        }
        if kind == "security-requirement" {
            for (name, scopes) in members {
                let ok = matches!(scopes, Doc::Seq(items) if items.iter().all(|i| matches!(i, Doc::Str(_))));
                if !ok {
                    return refuse(d, &at(path, name), "WrongType");
                }
                self.keep(
                    &id,
                    "scopes",
                    format!("{{{}:{}}}", json_string(name), json(scopes)),
                );
                self.requirements.push((d, path.to_owned(), name.clone()));
            }
        } else {
            for (key, value) in members {
                if key.starts_with("x-") {
                    self.keep(
                        &id,
                        "extension",
                        format!("{{{}:{}}}", json_string(key), json(value)),
                    );
                }
            }
        }
        Ok(id)
    }

    fn reference(&mut self, r: &Doc, path: &str, expected: &'static str) -> R<()> {
        let Doc::Str(t) = r else {
            return refuse(self.d, &at(path, "$ref"), "WrongType");
        };
        self.pending
            .push((self.d, path.to_owned(), t.clone(), expected));
        Ok(())
    }

    fn object_or_ref(&mut self, kind: &'static str, node: &'d Doc, path: &str) -> R<String> {
        let Some(r) = get(node, "$ref") else {
            return self.object(kind, node, path);
        };
        let d = self.d;
        let id = self.id(path);
        self.reference(r, path, kind)?;
        self.out.vertices.insert(id.clone(), "reference".to_owned());
        self.slots.insert((d, path.to_owned()), (kind, true, node));
        for f in &REFERENCE_FIELDS {
            if let Some(value) = get(node, f.0) {
                self.field(&id, f, value, &at(path, f.0))?;
            }
        }
        if let Doc::Map(members) = node {
            for (key, value) in members {
                if key != "$ref" && key != "summary" && key != "description" {
                    self.keep(
                        &id,
                        "ignored",
                        format!("{{{}:{}}}", json_string(key), json(value)),
                    );
                }
            }
        }
        Ok(id)
    }

    fn field(&mut self, id: &str, f: &F, value: &'d Doc, path: &str) -> R<()> {
        let d = self.d;
        let (name, h, _, edge) = *f;
        let fits = match h {
            H::Str => matches!(value, Doc::Str(_)),
            H::Bool => matches!(value, Doc::Bool(_)),
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
        match h {
            H::Str | H::Bool | H::Any | H::Strs | H::StrMap | H::AnyMap => {
                self.keep(id, name, json(value));
            }
            H::One(kind) => {
                let target = self.object(kind, value, path)?;
                self.edge(id, edge, &target, None);
            }
            H::OneRef(kind) => {
                let target = self.object_or_ref(kind, value, path)?;
                self.edge(id, edge, &target, None);
            }
            H::Schema => self.schema(id, edge, None, value, path),
            H::List(kind) | H::ListRef(kind) => {
                let Doc::Seq(items) = value else {
                    return refuse(d, path, "WrongType");
                };
                if items.is_empty() {
                    self.keep(id, name, "[]".to_owned());
                }
                let mut places = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let p = format!("{path}/{i}");
                    let target = if matches!(h, H::ListRef(_)) {
                        self.object_or_ref(kind, item, &p)?
                    } else {
                        self.object(kind, item, &p)?
                    };
                    self.edge(id, edge, &target, Some(i.to_string()));
                    places.push(p);
                }
                if name == "parameters" {
                    self.lists.push((d, path.to_owned(), places));
                }
            }
            H::Map(_) | H::MapRef(_) | H::Schemas => {
                let Doc::Map(entries) = value else {
                    return refuse(d, path, "WrongType");
                };
                if entries.is_empty() {
                    self.keep(id, name, "{}".to_owned());
                }
                let component = path.starts_with("/components/") && path.matches('/').count() == 2;
                for (key, entry) in entries {
                    let p = at(path, key);
                    let named = !key.is_empty()
                        && key
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
                    if component && !named {
                        return refuse(d, &p, "ComponentName");
                    }
                    let target = match h {
                        H::Map(kind) => self.object(kind, entry, &p)?,
                        H::MapRef(kind) => self.object_or_ref(kind, entry, &p)?,
                        _ => {
                            self.schema(id, edge, Some(key.clone()), entry, &p);
                            continue;
                        }
                    };
                    self.edge(id, edge, &target, Some(key.clone()));
                }
            }
            H::Pat(..) => {}
        }
        Ok(())
    }

    fn schema(
        &mut self,
        id: &str,
        edge: &'static str,
        name: Option<String>,
        node: &'d Doc,
        path: &str,
    ) {
        self.schemas.push((self.d, path.to_owned(), node));
        let target = self.id(path);
        self.schema_edges.push((id.to_owned(), edge, name, target));
    }

    fn patterned(
        &mut self,
        kind: &str,
        id: &str,
        members: &'d [(String, Doc)],
        path: &str,
        f: &F,
    ) -> R<()> {
        let H::Pat(target, or_ref) = f.1 else {
            return Ok(());
        };
        let edge = f.3;
        let d = self.d;
        let mut any = false;
        for (key, value) in members.iter().filter(|(k, _)| !k.starts_with("x-")) {
            let p = at(path, key);
            if (kind == "paths" && !key.starts_with('/'))
                || (kind == "responses" && !status_key(key))
            {
                return refuse(d, &p, "Invalid");
            }
            any = true;
            let vertex = if or_ref {
                self.object_or_ref(target, value, &p)?
            } else {
                self.object(target, value, &p)?
            };
            self.edge(id, edge, &vertex, Some(key.clone()));
        }
        if kind == "responses" && !any {
            return refuse(d, path, "Invalid");
        }
        Ok(())
    }
}

const WITH_SCHEMA: [&str; 5] = ["style", "explode", "allowReserved", "example", "examples"];

impl<'d> Rd<'d> {
    /// The rules of one object its field table does not state, in the
    /// page's order.
    fn rules(&self, kind: &str, node: &Doc, path: &str) -> R<()> {
        let d = self.d;
        let has = |k: &str| get(node, k).is_some();
        let both = |a: &str, b: &str| -> R<()> {
            if has(a) && has(b) {
                return refuse(d, &at(path, b), "Exclusive");
            }
            Ok(())
        };
        match kind {
            "openapi" => {
                if !has("paths") && !has("components") && !has("webhooks") {
                    return refuse(d, path, "Missing");
                }
                if let Some(Doc::Seq(tags)) = get(node, "tags") {
                    let mut names = BTreeSet::new();
                    for (i, tag) in tags.iter().enumerate() {
                        if let Some(name) = text(get(tag, "name"))
                            && !names.insert(name)
                        {
                            return refuse(d, &format!("{path}/tags/{i}/name"), "DuplicateTag");
                        }
                    }
                }
            }
            "license" => both("identifier", "url")?,
            "server" => {
                if text(get(node, "url")).is_some_and(|u| u.contains('?') || u.contains('#')) {
                    return refuse(d, &at(path, "url"), "Invalid");
                }
            }
            "server-variable" => {
                if let Some(Doc::Seq(values)) = get(node, "enum") {
                    if values.is_empty() {
                        return refuse(d, &at(path, "enum"), "Invalid");
                    }
                    if let Some(default) = text(get(node, "default"))
                        && !values
                            .iter()
                            .any(|v| matches!(v, Doc::Str(s) if s == default))
                    {
                        return refuse(d, &at(path, "default"), "Invalid");
                    }
                }
            }
            "parameter" => self.parameter_rules(node, path)?,
            "header" => {
                self.strategy(node, path)?;
                if text(get(node, "style")).is_some_and(|s| s != "simple") {
                    return refuse(d, &at(path, "style"), "Invalid");
                }
            }
            "media-type" => both("example", "examples")?,
            "encoding" => {
                if text(get(node, "style")).is_some_and(|s| !styles("query").contains(&s)) {
                    return refuse(d, &at(path, "style"), "Invalid");
                }
            }
            "example" => both("value", "externalValue")?,
            "link" => {
                both("operationRef", "operationId")?;
                if !has("operationRef") && !has("operationId") {
                    return refuse(d, path, "Missing");
                }
            }
            "security-scheme" => self.scheme_rules(node, path)?,
            "oauth-flows" => {
                for (flow, required, forbidden) in [
                    ("implicit", &["authorizationUrl"][..], "tokenUrl"),
                    ("password", &["tokenUrl"][..], "authorizationUrl"),
                    ("clientCredentials", &["tokenUrl"][..], "authorizationUrl"),
                    (
                        "authorizationCode",
                        &["authorizationUrl", "tokenUrl"][..],
                        "",
                    ),
                ] {
                    let Some(value @ Doc::Map(_)) = get(node, flow) else {
                        continue;
                    };
                    let p = at(path, flow);
                    if required.iter().any(|r| get(value, r).is_none()) {
                        return refuse(d, &p, "Missing");
                    }
                    if get(value, forbidden).is_some() {
                        return refuse(d, &at(&p, forbidden), "Invalid");
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn strategy(&self, node: &Doc, path: &str) -> R<()> {
        let d = self.d;
        match (get(node, "schema"), get(node, "content")) {
            (Some(_), Some(_)) => return refuse(d, &at(path, "content"), "Exclusive"),
            (None, None) => return refuse(d, path, "Missing"),
            (None, Some(content)) => {
                if matches!(content, Doc::Map(m) if m.len() != 1) {
                    return refuse(d, &at(path, "content"), "Invalid");
                }
                if let Some(name) = WITH_SCHEMA.iter().find(|n| get(node, n).is_some()) {
                    return refuse(d, &at(path, name), "Invalid");
                }
            }
            (Some(_), None) => {}
        }
        if get(node, "example").is_some() && get(node, "examples").is_some() {
            return refuse(d, &at(path, "examples"), "Exclusive");
        }
        Ok(())
    }

    fn parameter_rules(&self, node: &Doc, path: &str) -> R<()> {
        let d = self.d;
        let location = text(get(node, "in"));
        if location.is_some_and(|l| styles(l).is_empty()) {
            return refuse(d, &at(path, "in"), "Invalid");
        }
        self.strategy(node, path)?;
        if location.is_some_and(|l| l != "query") {
            for name in ["allowEmptyValue", "allowReserved"] {
                if get(node, name).is_some() {
                    return refuse(d, &at(path, name), "Invalid");
                }
            }
        }
        if let (Some(l), Some(style)) = (location, text(get(node, "style")))
            && !styles(l).contains(&style)
        {
            return refuse(d, &at(path, "style"), "Invalid");
        }
        if location == Some("path") && get(node, "required") != Some(&Doc::Bool(true)) {
            return refuse(d, path, "Invalid");
        }
        Ok(())
    }

    fn scheme_rules(&self, node: &Doc, path: &str) -> R<()> {
        let d = self.d;
        let Some(kind) = text(get(node, "type")) else {
            return Ok(());
        };
        let (allowed, required): (&[&str], &[&str]) = match kind {
            "apiKey" => (&["name", "in"], &["name", "in"]),
            "http" => (&["scheme", "bearerFormat"], &["scheme"]),
            "mutualTLS" => (&[], &[]),
            "oauth2" => (&["flows"], &["flows"]),
            "openIdConnect" => (&["openIdConnectUrl"], &["openIdConnectUrl"]),
            _ => return refuse(d, &at(path, "type"), "Invalid"),
        };
        for name in [
            "name",
            "in",
            "scheme",
            "bearerFormat",
            "flows",
            "openIdConnectUrl",
        ] {
            if get(node, name).is_some() && !allowed.contains(&name) {
                return refuse(d, &at(path, name), "Invalid");
            }
        }
        if required.iter().any(|r| get(node, r).is_none()) {
            return refuse(d, path, "Missing");
        }
        if kind == "apiKey"
            && text(get(node, "in")).is_some_and(|l| !["query", "header", "cookie"].contains(&l))
        {
            return refuse(d, &at(path, "in"), "Invalid");
        }
        if get(node, "bearerFormat").is_some()
            && text(get(node, "scheme")).is_none_or(|s| !s.eq_ignore_ascii_case("bearer"))
        {
            return refuse(d, &at(path, "bearerFormat"), "Invalid");
        }
        Ok(())
    }

    /// The document and pointer a reference made in document `d` names, or
    /// the refusal's class.
    fn locate(
        uris: &[Option<String>],
        d: usize,
        reference: &str,
    ) -> Result<(usize, String), &'static str> {
        let (doc, fragment) = if let Some(f) = reference.strip_prefix('#') {
            (d, f.to_owned())
        } else {
            let full = resolve(uris[d].as_deref(), reference).ok_or("External")?;
            let (uri, fragment) = match full.find('#') {
                Some(i) => (full[..i].to_owned(), full[i + 1..].to_owned()),
                None => (full, String::new()),
            };
            let doc = uris
                .iter()
                .position(|u| u.as_deref() == Some(uri.as_str()))
                .ok_or("External")?;
            (doc, fragment)
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

    fn references(&mut self, uris: &[Option<String>]) -> R<()> {
        let pending = std::mem::take(&mut self.pending);
        for (d, path, reference, expected) in &pending {
            let key = at(path, "$ref");
            let place =
                Self::locate(uris, *d, reference).or_else(|class| refuse(*d, &key, class))?;
            match self.slots.get(&place) {
                None => return refuse(*d, &key, "RefTarget"),
                Some((kind, _, _)) if kind != expected => return refuse(*d, &key, "RefKind"),
                Some(_) => {}
            }
            let src = format!("{}#{path}", self.prefix(*d));
            let tgt = format!("{}#{}", self.prefix(place.0), place.1);
            self.edge(&src, "ref", &tgt, Some(reference.clone()));
            self.targets.insert((*d, path.clone()), place);
        }
        for (d, path, _, _) in &pending {
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
        let read = js::read_embedded(uris, &self.oas, &self.schemas).map_err(|r| Refusal {
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
        Ok(())
    }

    fn of_kind(&self, kind: &str) -> Vec<(usize, String, &'d Doc)> {
        self.order
            .iter()
            .filter(|(k, _, _)| *k == kind)
            .map(|(_, d, p)| (*d, p.clone(), self.slots[&(*d, p.clone())].2))
            .collect()
    }

    fn operation_ids(&self) -> R<BTreeMap<String, (usize, String)>> {
        let mut ids = BTreeMap::new();
        for (d, path, node) in self.of_kind("operation") {
            if let Some(id) = text(get(node, "operationId")) {
                if ids.contains_key(id) {
                    return refuse(d, &at(&path, "operationId"), "DuplicateOperationId");
                }
                ids.insert(id.to_owned(), (d, path));
            }
        }
        Ok(ids)
    }

    fn links(&mut self, ids: &BTreeMap<String, (usize, String)>, uris: &[Option<String>]) -> R<()> {
        for (d, path, node) in self.of_kind("link") {
            let target = if let Some(value) = get(node, "operationId") {
                let name = text(Some(value)).unwrap_or_default();
                let Some(target) = ids.get(name) else {
                    return refuse(d, &at(&path, "operationId"), "UnknownOperationId");
                };
                target.clone()
            } else if let Some(value) = get(node, "operationRef") {
                let key = at(&path, "operationRef");
                let place = Self::locate(uris, d, text(Some(value)).unwrap_or_default())
                    .or_else(|class| refuse(d, &key, class))?;
                match self.slots.get(&place) {
                    Some(("operation", _, _)) => {}
                    Some(_) => return refuse(d, &key, "RefKind"),
                    None => return refuse(d, &key, "RefTarget"),
                }
                place
            } else {
                continue;
            };
            let src = format!("{}#{path}", self.prefix(d));
            let tgt = format!("{}#{}", self.prefix(target.0), target.1);
            self.edge(&src, "operation", &tgt, None);
        }
        Ok(())
    }

    fn security(&mut self) -> R<()> {
        for (d, path, name) in std::mem::take(&mut self.requirements) {
            let target = at("/components/securitySchemes", &name);
            if !matches!(
                self.slots.get(&(d, target.clone())),
                Some(("security-scheme", _, _))
            ) {
                return refuse(d, &at(&path, &name), "UnknownScheme");
            }
            let src = format!("{}#{path}", self.prefix(d));
            let tgt = format!("{}#{target}", self.prefix(d));
            self.edge(&src, "requirement", &tgt, Some(name));
        }
        Ok(())
    }

    /// The name and location of the parameter at a place, through its
    /// references.
    fn parameter(&self, d: usize, path: &str) -> Option<(&'d str, &'d str)> {
        let mut place = (d, path.to_owned());
        loop {
            let (_, is_reference, node) = self.slots.get(&place)?;
            if !is_reference {
                return Some((text(get(node, "name"))?, text(get(node, "in"))?));
            }
            place = self.targets.get(&place)?.clone();
        }
    }

    fn parameter_lists(&self) -> R<()> {
        for (d, _, items) in &self.lists {
            let mut seen = BTreeSet::new();
            for item in items {
                if let Some(pair) = self.parameter(*d, item)
                    && !seen.insert(pair)
                {
                    return refuse(*d, item, "DuplicateParameter");
                }
            }
        }
        Ok(())
    }

    fn path_parameters(&self, d: usize, list: &str) -> Vec<(&'d str, String)> {
        self.lists
            .iter()
            .filter(|(doc, p, _)| *doc == d && p == list)
            .flat_map(|(_, _, items)| items)
            .filter_map(|item| match self.parameter(d, item) {
                Some((name, "path")) => Some((name, item.clone())),
                _ => None,
            })
            .collect()
    }

    fn paths(&self) -> R<()> {
        for (d, (_, root)) in self.docs.iter().enumerate() {
            let Some(Doc::Map(paths)) = get(root, "paths") else {
                continue;
            };
            let mut shapes = BTreeSet::new();
            for (path, _) in paths.iter().filter(|(k, _)| k.starts_with('/')) {
                let p = at("/paths", path);
                let Some(names) = templates(path) else {
                    return refuse(d, &p, "Template");
                };
                if !shapes.insert(shape(path)) {
                    return refuse(d, &p, "DuplicatePath");
                }
                self.path_item(d, &p, &names)?;
            }
        }
        Ok(())
    }

    fn path_item(&self, d: usize, path: &str, names: &[String]) -> R<()> {
        let mut chain = vec![(d, path.to_owned())];
        while let Some(next) = self.targets.get(&chain[chain.len() - 1]) {
            chain.push(next.clone());
        }
        let mut shared = Vec::new();
        let mut operations = Vec::new();
        let mut methods = BTreeSet::new();
        for (doc, item) in &chain {
            for (name, place) in self.path_parameters(*doc, &at(item, "parameters")) {
                shared.push((*doc, name, place));
            }
            for method in METHODS {
                let op = at(item, method);
                if self.slots.contains_key(&(*doc, op.clone())) && methods.insert(method) {
                    operations.push((*doc, op));
                }
            }
        }
        for (doc, name, place) in &shared {
            if !names.iter().any(|n| n == name) {
                return refuse(*doc, place, "UnknownPathParameter");
            }
        }
        for (doc, op) in &operations {
            let own = self.path_parameters(*doc, &at(op, "parameters"));
            if let Some((_, place)) = own.iter().find(|(n, _)| !names.iter().any(|t| t == n)) {
                return refuse(*doc, place, "UnknownPathParameter");
            }
            if names
                .iter()
                .any(|t| !own.iter().any(|(n, _)| n == t) && !shared.iter().any(|(_, n, _)| n == t))
            {
                return refuse(*doc, op, "MissingPathParameter");
            }
        }
        Ok(())
    }
}

/// The canonical text of an `OpenAPI` document, followed by a line feed.
///
/// It holds each object's fields in the order of the specification's tables (a Path
/// Item's `$ref` first), its patterned fields in source order, its
/// extensions last; a Reference Object's `$ref`, `summary`, `description`
/// and other members; a Schema Object as the JSON Schema reference writes
/// it.
#[must_use]
pub fn canonical_text(doc: &Doc) -> String {
    format!("{}\n", canonical("openapi", doc))
}

fn canonical_or_ref(kind: &str, node: &Doc) -> String {
    let (Some(r), Doc::Map(members)) = (get(node, "$ref"), node) else {
        return canonical(kind, node);
    };
    let mut parts = vec![format!("\"$ref\":{}", json(r))];
    for name in ["summary", "description"] {
        if let Some(value) = get(node, name) {
            parts.push(format!("{}:{}", json_string(name), json(value)));
        }
    }
    for (k, value) in members {
        if k != "$ref" && k != "summary" && k != "description" {
            parts.push(format!("{}:{}", json_string(k), json(value)));
        }
    }
    format!("{{{}}}", parts.join(","))
}

fn canonical_schema(node: &Doc) -> String {
    let text = js::canonical_text(node);
    text.trim_end_matches('\n').to_owned()
}

fn canonical(kind: &str, node: &Doc) -> String {
    let Doc::Map(members) = node else {
        return json(node);
    };
    let mut parts = Vec::new();
    if kind == "path-item"
        && let Some(r) = get(node, "$ref")
    {
        parts.push(format!("\"$ref\":{}", json(r)));
    }
    for (name, h, _, _) in fields(kind) {
        let entry = |k: &str, value: String| format!("{}:{value}", json_string(k));
        if let H::Pat(target, or_ref) = h {
            for (k, value) in members.iter().filter(|(k, _)| !k.starts_with("x-")) {
                let written = if or_ref {
                    canonical_or_ref(target, value)
                } else {
                    canonical(target, value)
                };
                parts.push(entry(k, written));
            }
            continue;
        }
        let Some(value) = get(node, name) else {
            continue;
        };
        let written = match (h, value) {
            (H::One(k), v) => canonical(k, v),
            (H::OneRef(k), v) => canonical_or_ref(k, v),
            (H::Schema, v) => canonical_schema(v),
            (H::List(k) | H::ListRef(k), Doc::Seq(items)) => format!(
                "[{}]",
                items
                    .iter()
                    .map(|i| if matches!(h, H::ListRef(_)) {
                        canonical_or_ref(k, i)
                    } else {
                        canonical(k, i)
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            (H::Map(_) | H::MapRef(_) | H::Schemas, Doc::Map(entries)) => format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(k, v)| {
                        let w = match h {
                            H::Map(t) => canonical(t, v),
                            H::MapRef(t) => canonical_or_ref(t, v),
                            _ => canonical_schema(v),
                        };
                        entry(k, w)
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            (_, v) => json(v),
        };
        parts.push(entry(name, written));
    }
    for (k, value) in members {
        if kind == "security-requirement" || k.starts_with("x-") {
            parts.push(format!("{}:{}", json_string(k), json(value)));
        }
    }
    format!("{{{}}}", parts.join(","))
}
