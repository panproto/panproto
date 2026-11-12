//! Complete reference operations assigned to `json-schema`.
//!
//! A JSON Schema document is modelled as the data tree it denotes
//! ([`Doc`]): objects with their members in order, arrays, and the scalars
//! null, booleans, integers and strings. [`read`] applies the rules of the
//! `json-schema` page, transcribed from JSON Schema 2020-12 (its core and
//! validation specifications and meta-schemas), in the order the page
//! states them, to a bundle of documents each with an optional retrieval
//! URI, and returns either the first refusal or the [`Reading`]: the
//! schema's vertices, edges, kept constraints, required properties, entries
//! and document roots. [`resolve`] is RFC 3986 reference resolution, and
//! [`canonical_text`] the canonical document the page defines. It shares no
//! code with production, which reads JSON text through the tree-sitter JSON
//! grammar and builds a `panproto_schema::Schema`.

use std::collections::{BTreeMap, BTreeSet};

/// A data tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Doc {
    /// `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A string.
    Str(String),
    /// An array.
    Seq(Vec<Self>),
    /// An object, members in order, names distinct.
    Map(Vec<(String, Self)>),
}

/// A string node.
#[must_use]
pub fn s(text: &str) -> Doc {
    Doc::Str(text.to_owned())
}

/// An object node.
#[must_use]
pub fn m(entries: &[(&str, Doc)]) -> Doc {
    Doc::Map(
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    )
}

/// What is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A value of the wrong JSON kind: `a schema`, `a string`, `a boolean`,
    /// `a number`, `a non-negative integer`, `an array`, `an object` or `a
    /// string or an array`.
    WrongType(&'static str),
    /// A rule of the meta-schema is broken.
    Invalid,
    /// `$schema` names another dialect.
    UnsupportedDialect,
    /// A keyword of an earlier draft.
    Superseded(&'static str),
    /// A `$dynamicRef` whose target depends on the dynamic scope.
    DynamicScope,
    /// A resource URI given twice.
    DuplicateId,
    /// An anchor declared twice in one resource.
    DuplicateAnchor,
    /// A relative `$id` or reference with no base in scope.
    NoBase,
    /// A reference to a resource outside the bundle.
    RemoteRef,
    /// A pointer naming no subschema.
    RefTarget,
    /// An anchor its resource does not declare.
    UnknownAnchor,
    /// A retrieval URI given twice, or two documents without one.
    DuplicateDocument,
}

/// A refusal: the document, the JSON pointer of the value at fault, and
/// what is wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The document's index in the bundle.
    pub document: usize,
    /// The JSON pointer from the document root; empty for the root.
    pub path: String,
    /// What is wrong.
    pub kind: Kind,
}

/// One edge out of a vertex: its kind, its target and its name.
pub type OutEdge = (String, String, Option<String>);

/// What a bundle reads as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// Each vertex id with its kind.
    pub vertices: BTreeMap<String, String>,
    /// Each vertex's out-edges, in the order they are added.
    pub edges: BTreeMap<String, Vec<OutEdge>>,
    /// Each vertex's kept constraints, sort and JSON text, in order.
    pub constraints: BTreeMap<String, Vec<(String, String)>>,
    /// Each vertex's required property names that its `properties` declare.
    pub required: BTreeMap<String, BTreeSet<String>>,
    /// The entry vertices, in the order they are declared, each once.
    pub entries: Vec<String>,
    /// Each document's root vertex.
    pub roots: Vec<String>,
}

/// The one dialect a JSON Schema document is read under.
pub const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// The `OpenAPI` 3.1 dialect, which a Schema Object of an `OpenAPI`
/// document is read under unless it names 2020-12.
pub const OAS_DIALECT: &str = "https://spec.openapis.org/oas/3.1/dialect/base";

/// The keywords the `OpenAPI` vocabulary adds, kept under its dialect.
pub const OAS_KEYWORDS: [&str; 4] = ["discriminator", "xml", "externalDocs", "example"];

const OLD: [&str; 5] = [
    "definitions",
    "dependencies",
    "additionalItems",
    "$recursiveRef",
    "$recursiveAnchor",
];

const TYPE_NAMES: [&str; 7] = [
    "null", "boolean", "object", "array", "number", "string", "integer",
];

const KEPT: [&str; 36] = [
    "$schema",
    "$id",
    "$anchor",
    "$dynamicAnchor",
    "$vocabulary",
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
    "maxContains",
    "minContains",
    "maxProperties",
    "minProperties",
    "required",
    "dependentRequired",
    "format",
    "contentEncoding",
    "contentMediaType",
    "title",
    "description",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
    "examples",
];

/// The shape of an applicator's value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Single,
    Named,
    Indexed,
}

/// The applicators (and `$defs`) in the page's canonical order, each with
/// the shape of its value and its edge kind.
const SUBSCHEMA_KEYWORDS: [(&str, Shape, &str); 19] = [
    ("$defs", Shape::Named, "def"),
    ("properties", Shape::Named, "prop"),
    ("patternProperties", Shape::Named, "pattern-prop"),
    ("additionalProperties", Shape::Single, "additional-props"),
    ("propertyNames", Shape::Single, "property-names"),
    ("dependentSchemas", Shape::Named, "dependent-schema"),
    ("prefixItems", Shape::Indexed, "prefix-item"),
    ("items", Shape::Single, "items"),
    ("contains", Shape::Single, "contains"),
    ("allOf", Shape::Indexed, "all-of"),
    ("anyOf", Shape::Indexed, "any-of"),
    ("oneOf", Shape::Indexed, "one-of"),
    ("not", Shape::Single, "not"),
    ("if", Shape::Single, "if"),
    ("then", Shape::Single, "then"),
    ("else", Shape::Single, "else"),
    ("unevaluatedItems", Shape::Single, "unevaluated-items"),
    ("unevaluatedProperties", Shape::Single, "unevaluated-props"),
    ("contentSchema", Shape::Single, "content-schema"),
];

fn subschema_keyword(key: &str) -> Option<(Shape, &'static str)> {
    SUBSCHEMA_KEYWORDS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, shape, edge)| (*shape, *edge))
}

type R<T> = Result<T, Refusal>;

/// The JSON text of a string, escaped as RFC 8259 requires with the short
/// escapes for the five control characters that have one and `\u00xx` in
/// lowercase hexadecimal for the others.
#[must_use]
pub fn json_string(t: &str) -> String {
    let mut out = String::from('"');
    for c in t.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let digits = b"0123456789abcdef";
                let n = u32::from(c) as usize;
                out.push_str("\\u00");
                out.push(char::from(digits[n / 16]));
                out.push(char::from(digits[n % 16]));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The compact JSON text of a tree, members in order.
#[must_use]
pub fn json(doc: &Doc) -> String {
    match doc {
        Doc::Null => "null".into(),
        Doc::Bool(b) => b.to_string(),
        Doc::Int(i) => i.to_string(),
        Doc::Str(t) => json_string(t),
        Doc::Seq(items) => format!("[{}]", items.iter().map(json).collect::<Vec<_>>().join(",")),
        Doc::Map(members) => format!(
            "{{{}}}",
            members
                .iter()
                .map(|(k, v)| format!("{}:{}", json_string(k), json(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// The JSON text of a tree with a space after every `:` and `,` and a
/// line break after every object member, the input form the differential
/// reads.
#[must_use]
pub fn render(doc: &Doc) -> String {
    match doc {
        Doc::Seq(items) => format!(
            "[{}]",
            items.iter().map(render).collect::<Vec<_>>().join(", ")
        ),
        Doc::Map(members) => format!(
            "{{\n{}\n}}",
            members
                .iter()
                .map(|(k, v)| format!("{}: {}", json_string(k), render(v)))
                .collect::<Vec<_>>()
                .join(",\n")
        ),
        other => json(other),
    }
}

/// `key` as a JSON pointer token.
#[must_use]
pub fn token(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        match c {
            '~' => out.push_str("~0"),
            '/' => out.push_str("~1"),
            c => out.push(c),
        }
    }
    out
}

fn member<'d>(members: &'d [(String, Doc)], key: &str) -> Option<&'d Doc> {
    members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The parts of a URI reference by the regular expression of RFC 3986
/// Appendix B: scheme, authority, path, query, fragment.
type Parts<'u> = (
    Option<&'u str>,
    Option<&'u str>,
    &'u str,
    Option<&'u str>,
    Option<&'u str>,
);

fn split_reference(text: &str) -> Parts<'_> {
    let (rest, fragment) = text
        .find('#')
        .map_or((text, None), |i| (&text[..i], Some(&text[i + 1..])));
    let (rest, query) = rest
        .find('?')
        .map_or((rest, None), |i| (&rest[..i], Some(&rest[i + 1..])));
    let scheme_end = rest.find(':').filter(|i| {
        *i > 0
            && !rest[..*i].contains('/')
            && !rest[..*i].contains('?')
            && !rest[..*i].contains('#')
    });
    let (scheme, rest) = scheme_end.map_or((None, rest), |i| (Some(&rest[..i]), &rest[i + 1..]));
    let (authority, path) = rest.strip_prefix("//").map_or((None, rest), |after| {
        let end = after.find('/').unwrap_or(after.len());
        (Some(&after[..end]), &after[end..])
    });
    (scheme, authority, path, query, fragment)
}

/// RFC 3986 Section 5.2.4.
fn remove_dots(path: &str) -> String {
    let mut input = path.to_owned();
    let mut output = String::new();
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix("../") {
            input = rest.to_owned();
        } else if let Some(rest) = input.strip_prefix("./") {
            input = rest.to_owned();
        } else if input.starts_with("/./") {
            input = input[2..].to_owned();
        } else if input == "/." {
            "/".clone_into(&mut input);
        } else if input.starts_with("/../") || input == "/.." {
            input = if input == "/.." {
                "/".to_owned()
            } else {
                input[3..].to_owned()
            };
            match output.rfind('/') {
                Some(i) => output.truncate(i),
                None => output.clear(),
            }
        } else if input == "." || input == ".." {
            input.clear();
        } else {
            let start = usize::from(input.starts_with('/'));
            let end = input[start..].find('/').map_or(input.len(), |i| i + start);
            output.push_str(&input[..end]);
            input = input[end..].to_owned();
        }
    }
    output
}

/// Resolve `reference` against `base` by RFC 3986 Section 5.2.2, with no
/// normalization beyond dot-segment removal; `None` when the reference is
/// relative and there is no base, or the base has no scheme.
#[must_use]
pub fn resolve(base: Option<&str>, reference: &str) -> Option<String> {
    let (r_scheme, r_authority, r_path, r_query, r_fragment) = split_reference(reference);
    let (scheme, authority, path, query) = if let Some(scheme) = r_scheme {
        (scheme.to_owned(), r_authority, remove_dots(r_path), r_query)
    } else {
        let (b_scheme, b_authority, b_path, b_query, _) = split_reference(base?);
        let b_scheme = b_scheme?;
        if r_authority.is_some() {
            (
                b_scheme.to_owned(),
                r_authority,
                remove_dots(r_path),
                r_query,
            )
        } else if r_path.is_empty() {
            (
                b_scheme.to_owned(),
                b_authority,
                b_path.to_owned(),
                r_query.or(b_query),
            )
        } else if r_path.starts_with('/') {
            (
                b_scheme.to_owned(),
                b_authority,
                remove_dots(r_path),
                r_query,
            )
        } else {
            let merged = if b_authority.is_some() && b_path.is_empty() {
                format!("/{r_path}")
            } else {
                b_path.rfind('/').map_or_else(
                    || r_path.to_owned(),
                    |i| format!("{}{r_path}", &b_path[..=i]),
                )
            };
            (
                b_scheme.to_owned(),
                b_authority,
                remove_dots(&merged),
                r_query,
            )
        }
    };
    let mut out = format!("{scheme}:");
    if let Some(authority) = authority {
        out.push_str("//");
        out.push_str(authority);
    }
    out.push_str(&path);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = r_fragment {
        out.push('#');
        out.push_str(fragment);
    }
    Some(out)
}

/// The resource and base in scope at a subschema.
#[derive(Clone, Debug)]
struct Scope {
    resource: String,
    base: Option<String>,
    /// Whether the `OpenAPI` vocabulary is in effect.
    oas: bool,
}

/// What the first pass gathers.
#[derive(Default)]
struct Gathered {
    resources: BTreeMap<String, (usize, String)>,
    anchors: BTreeMap<(String, String), (usize, String, bool)>,
    dynamic_count: BTreeMap<String, usize>,
    scopes: BTreeMap<(usize, String), Scope>,
    prefixes: Vec<String>,
    /// Whether the documents are host documents holding Schema Objects,
    /// registered by the host, whose `$schema` may name the OAS dialect.
    hosted: bool,
}

fn refuse<T>(document: usize, path: &str, kind: Kind) -> R<T> {
    Err(Refusal {
        document,
        path: path.to_owned(),
        kind,
    })
}

fn at(path: &str, key: &str) -> String {
    format!("{path}/{}", token(key))
}

fn without_empty_fragment(uri: &str) -> &str {
    uri.strip_suffix('#').unwrap_or(uri)
}

/// Read a bundle as the page states.
///
/// # Errors
///
/// The first [`Refusal`], in the page's order.
pub fn read(documents: &[(Option<String>, Doc)]) -> Result<Reading, Refusal> {
    let mut g = Gathered::default();
    for (d, (uri, root)) in documents.iter().enumerate() {
        let start = match uri {
            None => Scope {
                resource: String::new(),
                base: None,
                oas: false,
            },
            Some(uri) => {
                let base = without_empty_fragment(uri);
                if base.contains('#') || resolve(None, base).is_none() {
                    return refuse(d, "", Kind::Invalid);
                }
                if g.resources.contains_key(base) {
                    return refuse(d, "", Kind::DuplicateDocument);
                }
                g.resources.insert(base.to_owned(), (d, String::new()));
                Scope {
                    resource: base.to_owned(),
                    base: Some(base.to_owned()),
                    oas: false,
                }
            }
        };
        let resource = first_pass(&mut g, d, root, "", &start, true)?;
        g.prefixes.push(
            uri.as_deref()
                .map_or(resource, |u| without_empty_fragment(u).to_owned()),
        );
    }
    let mut out = Reading::default();
    let mut refs = Vec::new();
    for (d, (_, root)) in documents.iter().enumerate() {
        let id = second_pass(&g, &mut out, &mut refs, d, root, "")?;
        if !out.entries.contains(&id) {
            out.entries.push(id.clone());
        }
        out.roots.push(id);
    }
    for (src, kind, tgt, name) in refs {
        out.edges
            .entry(src)
            .or_default()
            .push((kind.to_owned(), tgt, Some(name)));
    }
    Ok(out)
}

/// A Schema Object held by a host document: the document, the JSON
/// pointer of the Schema Object, and the Schema Object.
pub type Embedded<'d> = (usize, String, &'d Doc);

/// Read the Schema Objects of host documents, as the `OpenAPI` page states.
///
/// Each document is a resource identified by its URI (`uris`, or the empty
/// key for the one without), whose Schema Objects start under the `OpenAPI`
/// vocabulary when `oas` says so; references resolve among the Schema
/// Objects of every document.
///
/// # Errors
///
/// The first [`Refusal`], in the page's order.
pub fn read_embedded(
    uris: &[Option<String>],
    oas: &[bool],
    schemas: &[Embedded<'_>],
) -> Result<Reading, Refusal> {
    let mut g = Gathered {
        hosted: true,
        ..Gathered::default()
    };
    for (d, uri) in uris.iter().enumerate() {
        let key = uri.clone().unwrap_or_default();
        g.resources.insert(key.clone(), (d, String::new()));
        g.prefixes.push(key);
    }
    for (d, path, node) in schemas {
        let start = Scope {
            resource: uris[*d].clone().unwrap_or_default(),
            base: uris[*d].clone(),
            oas: oas[*d],
        };
        first_pass(&mut g, *d, node, path, &start, path.is_empty())?;
    }
    let mut out = Reading::default();
    let mut refs = Vec::new();
    for (d, path, node) in schemas {
        let id = second_pass(&g, &mut out, &mut refs, *d, node, path)?;
        out.roots.push(id);
    }
    for (src, kind, tgt, name) in refs {
        out.edges
            .entry(src)
            .or_default()
            .push((kind.to_owned(), tgt, Some(name)));
    }
    Ok(out)
}

/// Check an `OpenAPI` vocabulary keyword: `discriminator` an object with a
/// string `propertyName` and only `mapping` (an object of strings) and
/// `x-` members beside it; `xml` an object of string `name`, `namespace`
/// and `prefix`, boolean `attribute` and `wrapped`, and `x-` members;
/// `externalDocs` an object with `url`, of string `url` and `description`
/// and `x-` members.
fn check_oas_keyword(d: usize, key: &str, value: &Doc, path: &str) -> R<()> {
    let members = match (key, value) {
        ("discriminator" | "xml" | "externalDocs", Doc::Map(members)) => members,
        ("discriminator" | "xml" | "externalDocs", _) => return refuse(d, path, Kind::Invalid),
        _ => return Ok(()),
    };
    match (key, member(members, "propertyName"), member(members, "url")) {
        ("discriminator", None, _) | ("externalDocs", _, None) => {
            return refuse(d, path, Kind::Invalid);
        }
        ("discriminator", Some(name), _) if !matches!(name, Doc::Str(_)) => {
            return refuse(d, &at(path, "propertyName"), Kind::Invalid);
        }
        _ => {}
    }
    for (name, v) in members {
        let fits = match (key, name.as_str(), v) {
            (_, n, _) if n.starts_with("x-") => true,
            ("discriminator", "mapping", Doc::Map(entries)) => {
                entries.iter().all(|(_, s)| matches!(s, Doc::Str(_)))
            }
            ("discriminator", "propertyName", _)
            | ("xml", "name" | "namespace" | "prefix", Doc::Str(_))
            | ("xml", "attribute" | "wrapped", Doc::Bool(_))
            | ("externalDocs", "url" | "description", Doc::Str(_)) => true,
            _ => false,
        };
        if !fits {
            return refuse(d, &at(path, name), Kind::Invalid);
        }
    }
    Ok(())
}

/// The first pass over one subschema: checks, then `$schema`, then the
/// scope and resource, then anchors, then the subschemas it holds, keyword
/// by keyword in the canonical order.
fn first_pass(
    g: &mut Gathered,
    d: usize,
    node: &Doc,
    path: &str,
    outer: &Scope,
    root: bool,
) -> R<String> {
    let members = match node {
        Doc::Bool(_) => {
            let scope = scope_of(g, d, path, outer, root, None)?;
            let resource = scope.resource.clone();
            g.scopes.insert((d, path.to_owned()), scope);
            return Ok(resource);
        }
        Doc::Map(members) => members,
        _ => return refuse(d, path, Kind::WrongType("a schema")),
    };
    for (key, value) in members {
        check(d, key, value, &at(path, key))?;
    }
    let id = member(members, "$id");
    let mut oas = outer.oas;
    if let Some(dialect) = member(members, "$schema") {
        if !root && id.is_none() {
            return refuse(d, &at(path, "$schema"), Kind::Invalid);
        }
        if let Doc::Str(text) = dialect {
            let named = without_empty_fragment(text);
            if named == DIALECT {
                oas = false;
            } else if g.hosted && named == OAS_DIALECT {
                oas = true;
            } else {
                return refuse(d, &at(path, "$schema"), Kind::UnsupportedDialect);
            }
        }
    }
    if oas {
        for (key, value) in members {
            check_oas_keyword(d, key, value, &at(path, key))?;
        }
    }
    let mut scope = scope_of(g, d, path, outer, root, id)?;
    scope.oas = oas;
    for (key, value) in members {
        let dynamic = match key.as_str() {
            "$anchor" => false,
            "$dynamicAnchor" => true,
            _ => continue,
        };
        let Doc::Str(name) = value else { continue };
        let slot = (scope.resource.clone(), name.clone());
        if g.anchors.contains_key(&slot) {
            return refuse(d, &at(path, key), Kind::DuplicateAnchor);
        }
        g.anchors.insert(slot, (d, path.to_owned(), dynamic));
        if dynamic {
            *g.dynamic_count.entry(name.clone()).or_insert(0) += 1;
        }
    }
    g.scopes.insert((d, path.to_owned()), scope.clone());
    for (key, _, _) in SUBSCHEMA_KEYWORDS {
        let Some(value) = member(members, key) else {
            continue;
        };
        for (sub_path, sub) in subschemas(key, value, path) {
            first_pass(g, d, sub, &sub_path, &scope, false)?;
        }
    }
    Ok(scope.resource)
}

/// The subschemas `key` holds, each with its path and edge name.
fn subschemas<'d>(key: &str, value: &'d Doc, path: &str) -> Vec<(String, &'d Doc)> {
    named_subschemas(key, value, path)
        .into_iter()
        .map(|(p, _, sub)| (p, sub))
        .collect()
}

fn named_subschemas<'d>(
    key: &str,
    value: &'d Doc,
    path: &str,
) -> Vec<(String, Option<String>, &'d Doc)> {
    let here = at(path, key);
    match (subschema_keyword(key), value) {
        (Some((Shape::Single, _)), sub) => vec![(here, None, sub)],
        (Some((Shape::Named, _)), Doc::Map(members)) => members
            .iter()
            .map(|(name, sub)| (at(&here, name), Some(name.clone()), sub))
            .collect(),
        (Some((Shape::Indexed, _)), Doc::Seq(items)) => items
            .iter()
            .enumerate()
            .map(|(i, sub)| (format!("{here}/{i}"), Some(i.to_string()), sub))
            .collect(),
        _ => Vec::new(),
    }
}

/// The scope of a subschema, registering a new resource for `$id` and the
/// root of a document without a URI.
fn scope_of(
    g: &mut Gathered,
    d: usize,
    path: &str,
    outer: &Scope,
    root: bool,
    id: Option<&Doc>,
) -> R<Scope> {
    if let Some(Doc::Str(text)) = id {
        let text = without_empty_fragment(text);
        let resolved = resolve(outer.base.as_deref(), text);
        let Some(resolved) = resolved else {
            return refuse(d, &at(path, "$id"), Kind::NoBase);
        };
        match g.resources.get(&resolved) {
            Some((doc, place)) if *doc == d && place == path => {}
            Some(_) => return refuse(d, &at(path, "$id"), Kind::DuplicateId),
            None => {
                g.resources.insert(resolved.clone(), (d, path.to_owned()));
            }
        }
        return Ok(Scope {
            resource: resolved.clone(),
            base: Some(resolved),
            oas: outer.oas,
        });
    }
    if root && outer.base.is_none() && !g.hosted {
        if g.resources.contains_key("") {
            return refuse(d, path, Kind::DuplicateDocument);
        }
        g.resources.insert(String::new(), (d, String::new()));
    }
    Ok(outer.clone())
}

/// Whether `key` holds an object of subschemas and `value` is an empty
/// object, which is kept as a constraint rather than reached by edges.
fn empty_named(key: &str, value: &Doc) -> bool {
    matches!(subschema_keyword(key), Some((Shape::Named, _)))
        && matches!(value, Doc::Map(members) if members.is_empty())
}

const fn is_count(doc: &Doc) -> bool {
    matches!(doc, Doc::Int(n) if *n >= 0)
}

/// Check one keyword's value.
fn check(d: usize, key: &str, value: &Doc, path: &str) -> R<()> {
    if let Some(old) = OLD.iter().find(|o| **o == key) {
        return refuse(d, path, Kind::Superseded(old));
    }
    let bad = |kind| refuse(d, path, kind);
    match (key, value) {
        (
            "$schema" | "$ref" | "$dynamicRef" | "$comment" | "title" | "description" | "format"
            | "contentEncoding" | "contentMediaType" | "pattern",
            Doc::Str(_),
        )
        | ("enum" | "examples", Doc::Seq(_))
        | ("uniqueItems" | "deprecated" | "readOnly" | "writeOnly", Doc::Bool(_))
        | ("maximum" | "exclusiveMaximum" | "minimum" | "exclusiveMinimum", Doc::Int(_)) => Ok(()),
        (
            "$schema" | "$ref" | "$dynamicRef" | "$comment" | "title" | "description" | "format"
            | "contentEncoding" | "contentMediaType" | "pattern" | "$id" | "$anchor"
            | "$dynamicAnchor",
            v,
        ) if !matches!(v, Doc::Str(_)) => bad(Kind::WrongType("a string")),
        ("$id", Doc::Str(text)) => {
            if without_empty_fragment(text).contains('#') {
                bad(Kind::Invalid)
            } else {
                Ok(())
            }
        }
        ("$anchor" | "$dynamicAnchor", Doc::Str(text)) => {
            let ok = text.chars().enumerate().all(|(i, c)| {
                c.is_ascii_alphabetic()
                    || c == '_'
                    || (i > 0 && (c.is_ascii_digit() || c == '-' || c == '.'))
            }) && !text.is_empty();
            if ok { Ok(()) } else { bad(Kind::Invalid) }
        }
        ("enum" | "examples", _) => bad(Kind::WrongType("an array")),
        ("uniqueItems" | "deprecated" | "readOnly" | "writeOnly", _) => {
            bad(Kind::WrongType("a boolean"))
        }
        ("maximum" | "exclusiveMaximum" | "minimum" | "exclusiveMinimum", _) => {
            bad(Kind::WrongType("a number"))
        }
        ("multipleOf", Doc::Int(n)) => {
            if *n > 0 {
                Ok(())
            } else {
                bad(Kind::Invalid)
            }
        }
        ("multipleOf", _) => bad(Kind::WrongType("a number")),
        (
            "maxLength" | "minLength" | "maxItems" | "minItems" | "maxContains" | "minContains"
            | "maxProperties" | "minProperties",
            v,
        ) => {
            if is_count(v) {
                Ok(())
            } else {
                bad(Kind::WrongType("a non-negative integer"))
            }
        }
        ("type", v) => check_type(d, v, path),
        ("required", v) => check_distinct_strings(d, v, path),
        ("dependentRequired", Doc::Map(members)) => {
            for (name, v) in members {
                check_distinct_strings(d, v, &at(path, name))?;
            }
            Ok(())
        }
        ("$vocabulary", Doc::Map(members)) => {
            for (uri, flag) in members {
                if !matches!(flag, Doc::Bool(_)) {
                    return refuse(d, &at(path, uri), Kind::WrongType("a boolean"));
                }
            }
            Ok(())
        }
        ("dependentRequired" | "$vocabulary", _) => bad(Kind::WrongType("an object")),
        (k, v) => match (subschema_keyword(k), v) {
            (Some((Shape::Named, _)), Doc::Map(_)) | (Some((Shape::Single, _)) | None, _) => Ok(()),
            (Some((Shape::Named, _)), _) => bad(Kind::WrongType("an object")),
            (Some((Shape::Indexed, _)), Doc::Seq(items)) => {
                if items.is_empty() {
                    bad(Kind::Invalid)
                } else {
                    Ok(())
                }
            }
            (Some((Shape::Indexed, _)), _) => bad(Kind::WrongType("an array")),
        },
    }
}

fn check_type(d: usize, value: &Doc, path: &str) -> R<()> {
    match value {
        Doc::Str(name) if TYPE_NAMES.contains(&name.as_str()) => Ok(()),
        Doc::Str(_) => refuse(d, path, Kind::Invalid),
        Doc::Seq(items) if items.is_empty() => refuse(d, path, Kind::Invalid),
        Doc::Seq(items) => {
            let mut seen = BTreeSet::new();
            for (i, item) in items.iter().enumerate() {
                let item_path = format!("{path}/{i}");
                let Doc::Str(name) = item else {
                    return refuse(d, &item_path, Kind::WrongType("a string"));
                };
                if !TYPE_NAMES.contains(&name.as_str()) || !seen.insert(name) {
                    return refuse(d, &item_path, Kind::Invalid);
                }
            }
            Ok(())
        }
        _ => refuse(d, path, Kind::WrongType("a string or an array")),
    }
}

fn check_distinct_strings(d: usize, value: &Doc, path: &str) -> R<()> {
    let Doc::Seq(items) = value else {
        return refuse(d, path, Kind::WrongType("an array"));
    };
    let mut seen = BTreeSet::new();
    for (i, item) in items.iter().enumerate() {
        let Doc::Str(name) = item else {
            return refuse(d, &format!("{path}/{i}"), Kind::WrongType("a string"));
        };
        if !seen.insert(name) {
            return refuse(d, &format!("{path}/{i}"), Kind::Invalid);
        }
    }
    Ok(())
}

type PendingRef = (String, &'static str, String, String);

/// The second pass over one subschema: its vertex, its constraints, its
/// references, then its subschemas and their edges, keyword by keyword in
/// the canonical order, then its required properties.
fn second_pass(
    g: &Gathered,
    out: &mut Reading,
    refs: &mut Vec<PendingRef>,
    d: usize,
    node: &Doc,
    path: &str,
) -> R<String> {
    let id = format!("{}#{path}", g.prefixes[d]);
    let members = match node {
        Doc::Bool(true) => {
            out.vertices.insert(id.clone(), "true".into());
            return Ok(id);
        }
        Doc::Bool(false) => {
            out.vertices.insert(id.clone(), "false".into());
            return Ok(id);
        }
        Doc::Map(members) => members,
        _ => return refuse(d, path, Kind::WrongType("a schema")),
    };
    let kind = match member(members, "type") {
        None => "any".to_owned(),
        Some(Doc::Str(name)) => name.clone(),
        Some(_) => "union".to_owned(),
    };
    out.vertices.insert(id.clone(), kind);
    for (key, value) in members {
        if (subschema_keyword(key).is_some() && !empty_named(key, value))
            || key == "$ref"
            || key == "$dynamicRef"
        {
            continue;
        }
        let oas = g.scopes[&(d, path.to_owned())].oas && OAS_KEYWORDS.contains(&key.as_str());
        let (sort, text) = if KEPT.contains(&key.as_str()) || empty_named(key, value) || oas {
            (key.clone(), json(value))
        } else {
            (
                "annotation".to_owned(),
                format!("{{{}:{}}}", json_string(key), json(value)),
            )
        };
        out.constraints
            .entry(id.clone())
            .or_default()
            .push((sort, text));
    }
    for (key, edge) in [("$ref", "ref"), ("$dynamicRef", "dynamic-ref")] {
        if let Some(Doc::Str(reference)) = member(members, key) {
            let target = target_of(g, d, path, &at(path, key), reference, edge == "dynamic-ref")?;
            refs.push((id.clone(), edge, target, reference.clone()));
        }
    }
    for (key, _, edge) in SUBSCHEMA_KEYWORDS {
        let Some(value) = member(members, key) else {
            continue;
        };
        for (sub_path, name, sub) in named_subschemas(key, value, path) {
            let target = second_pass(g, out, refs, d, sub, &sub_path)?;
            if key == "$defs" && !out.entries.contains(&target) {
                out.entries.push(target.clone());
            }
            out.edges
                .entry(id.clone())
                .or_default()
                .push((edge.to_owned(), target, name));
        }
    }
    if let (Some(Doc::Seq(names)), Some(Doc::Map(props))) =
        (member(members, "required"), member(members, "properties"))
    {
        let declared: BTreeSet<String> = names
            .iter()
            .filter_map(|n| match n {
                Doc::Str(name) if props.iter().any(|(p, _)| p == name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        if !declared.is_empty() {
            out.required.insert(id.clone(), declared);
        }
    }
    Ok(id)
}

/// The vertex a reference written at `path` (the keyword's path `key_path`)
/// names.
fn target_of(
    g: &Gathered,
    d: usize,
    path: &str,
    key_path: &str,
    reference: &str,
    dynamic: bool,
) -> R<String> {
    let scope = &g.scopes[&(d, path.to_owned())];
    let (resource, fragment) = if let Some(fragment) = reference.strip_prefix('#') {
        (scope.resource.clone(), fragment.to_owned())
    } else {
        let Some(full) = resolve(scope.base.as_deref(), reference) else {
            return refuse(d, key_path, Kind::NoBase);
        };
        match full.find('#') {
            Some(i) => (full[..i].to_owned(), full[i + 1..].to_owned()),
            None => (full, String::new()),
        }
    };
    let Some((doc, root)) = g.resources.get(&resource) else {
        return refuse(d, key_path, Kind::RemoteRef);
    };
    let (doc, place, anchored_dynamically) = if fragment.is_empty() {
        (*doc, root.clone(), false)
    } else if fragment.starts_with('/') {
        let Some(pointer) = unescape_percent(&fragment) else {
            return refuse(d, key_path, Kind::RefTarget);
        };
        let place = format!("{root}{pointer}");
        if !g.scopes.contains_key(&(*doc, place.clone())) {
            return refuse(d, key_path, Kind::RefTarget);
        }
        (*doc, place, false)
    } else {
        let Some((doc, place, is_dynamic)) = g.anchors.get(&(resource.clone(), fragment.clone()))
        else {
            return refuse(d, key_path, Kind::UnknownAnchor);
        };
        (*doc, place.clone(), *is_dynamic)
    };
    if dynamic && anchored_dynamically && g.dynamic_count.get(&fragment).copied().unwrap_or(0) >= 2
    {
        return refuse(d, key_path, Kind::DynamicScope);
    }
    Ok(format!("{}#{place}", g.prefixes[doc]))
}

fn unescape_percent(text: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut rest = text.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        if first == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(first);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

/// The canonical text of a document, followed by a line feed.
///
/// A boolean schema is written as itself, and a schema object as its kept
/// and unknown keywords and its empty objects of subschemas in source
/// order, then `$ref` and `$dynamicRef`, then the applicators in the page's
/// order, each subschema canonical.
#[must_use]
pub fn canonical_text(doc: &Doc) -> String {
    format!("{}\n", canonical_schema(doc))
}

fn canonical_schema(doc: &Doc) -> String {
    let Doc::Map(members) = doc else {
        return json(doc);
    };
    let mut parts: Vec<String> = members
        .iter()
        .filter(|(k, v)| {
            (subschema_keyword(k).is_none() || empty_named(k, v))
                && k != "$ref"
                && k != "$dynamicRef"
        })
        .map(|(k, v)| format!("{}:{}", json_string(k), json(v)))
        .collect();
    for key in ["$ref", "$dynamicRef"] {
        if let Some(v) = member(members, key) {
            parts.push(format!("{}:{}", json_string(key), json(v)));
        }
    }
    for (key, shape, _) in SUBSCHEMA_KEYWORDS {
        let Some(value) = member(members, key).filter(|v| !empty_named(key, v)) else {
            continue;
        };
        let written = match (shape, value) {
            (Shape::Named, Doc::Map(entries)) => format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(k, v)| format!("{}:{}", json_string(k), canonical_schema(v)))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            (Shape::Indexed, Doc::Seq(items)) => format!(
                "[{}]",
                items
                    .iter()
                    .map(canonical_schema)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            (_, v) => canonical_schema(v),
        };
        parts.push(format!("{}:{written}", json_string(key)));
    }
    format!("{{{}}}", parts.join(","))
}
