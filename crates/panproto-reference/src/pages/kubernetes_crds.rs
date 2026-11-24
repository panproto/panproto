//! Complete reference operations assigned to `kubernetes-crds`.
//!
//! A `CustomResourceDefinition` is modelled as the data tree it denotes
//! ([`Doc`]): mappings with their entries in order, sequences, and the
//! scalars null, booleans, integers and strings. [`read`] applies the rules
//! of the `kubernetes-crds` page, transcribed from the `apiextensions.k8s.io/v1`
//! definition of Kubernetes 1.37, in the order the page states them, and
//! returns either the first refusal or the [`Reading`]: the schema's vertices,
//! edges, kept constraints, required properties and list-map keys, and the
//! `required` entries that resolve to nothing. [`canonical_text`] is the
//! canonical CRD the page defines. It shares no code with production, which
//! reads YAML text through the tree-sitter YAML grammar and builds a
//! `panproto_schema::Schema`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

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
    /// A sequence.
    Seq(Vec<Self>),
    /// A mapping, entries in order, keys distinct.
    Map(Vec<(String, Self)>),
}

/// A string node.
#[must_use]
pub fn s(text: &str) -> Doc {
    Doc::Str(text.to_owned())
}

/// A mapping node.
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
    /// `apiVersion` is not `apiextensions.k8s.io/v1`.
    ApiVersion,
    /// `kind` is not `CustomResourceDefinition`.
    Kind,
    /// A required field is absent.
    Missing(&'static str),
    /// A value of the wrong kind: `a mapping`, `a sequence`, `a string`,
    /// `a boolean`, `a number` or `a non-negative integer`.
    WrongType(&'static str),
    /// A rule of the page is broken.
    Invalid,
    /// A field the definition does not declare.
    UnknownField(String),
    /// A schema field Kubernetes refuses.
    Forbidden(&'static str),
    /// A feature outside the supported set.
    Unsupported(&'static str),
    /// A repeated version name.
    DuplicateVersion(String),
    /// The number of storage versions, when it is not one.
    StorageVersions(usize),
    /// A list-map key that names no scalar property of the items.
    ListMapKey(String),
}

/// A refusal: the path of the field at fault, and what is wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// Keys joined by `.`, indices as `[i]`; empty for the document.
    pub path: String,
    /// What is wrong.
    pub kind: Kind,
}

/// One edge out of a vertex: its kind, its target and its name.
pub type OutEdge = (String, String, Option<String>);

/// What a CRD reads as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// Each vertex id with its kind.
    pub vertices: BTreeMap<String, String>,
    /// Each vertex's out-edges, in the order they are added.
    pub edges: BTreeMap<String, Vec<OutEdge>>,
    /// Each vertex's kept constraints, sort and JSON text, in order.
    pub constraints: BTreeMap<String, Vec<(String, String)>>,
    /// Each object vertex's required property names that resolve.
    pub required: BTreeMap<String, BTreeSet<String>>,
    /// Each list map's items vertex, with the list vertex and the key names.
    pub keys: BTreeMap<String, (String, Vec<String>)>,
    /// The unresolved `required` entries: path and name, in order.
    pub unresolved: Vec<(String, String)>,
}

/// The API version read.
pub const API_VERSION: &str = "apiextensions.k8s.io/v1";

const TYPES: [&str; 6] = ["object", "array", "string", "integer", "number", "boolean"];

const FORBIDDEN: [&str; 7] = [
    "$ref",
    "$schema",
    "id",
    "definitions",
    "dependencies",
    "additionalItems",
    "patternProperties",
];

const KEPT: [&str; 33] = [
    "description",
    "title",
    "format",
    "default",
    "example",
    "enum",
    "nullable",
    "maximum",
    "exclusiveMaximum",
    "minimum",
    "exclusiveMinimum",
    "multipleOf",
    "maxLength",
    "minLength",
    "pattern",
    "maxItems",
    "minItems",
    "uniqueItems",
    "maxProperties",
    "minProperties",
    "required",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "externalDocs",
    "x-kubernetes-preserve-unknown-fields",
    "x-kubernetes-embedded-resource",
    "x-kubernetes-int-or-string",
    "x-kubernetes-list-type",
    "x-kubernetes-list-map-keys",
    "x-kubernetes-map-type",
    "x-kubernetes-validations",
];

const METADATA: [&str; 15] = [
    "name",
    "generateName",
    "namespace",
    "labels",
    "annotations",
    "uid",
    "resourceVersion",
    "generation",
    "creationTimestamp",
    "deletionTimestamp",
    "deletionGracePeriodSeconds",
    "ownerReferences",
    "finalizers",
    "managedFields",
    "selfLink",
];

type R<T> = Result<T, Refusal>;

fn refuse<T>(path: &str, kind: Kind) -> R<T> {
    Err(Refusal {
        path: path.to_owned(),
        kind,
    })
}

fn at(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn field<'d>(entries: &'d [(String, Doc)], key: &str) -> Option<&'d Doc> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn map<'d>(doc: &'d Doc, path: &str) -> R<&'d [(String, Doc)]> {
    match doc {
        Doc::Map(entries) => Ok(entries),
        _ => refuse(path, Kind::WrongType("a mapping")),
    }
}

fn seq<'d>(doc: &'d Doc, path: &str) -> R<&'d [Doc]> {
    match doc {
        Doc::Seq(items) => Ok(items),
        _ => refuse(path, Kind::WrongType("a sequence")),
    }
}

fn text<'d>(doc: &'d Doc, path: &str) -> R<&'d str> {
    match doc {
        Doc::Str(t) => Ok(t),
        _ => refuse(path, Kind::WrongType("a string")),
    }
}

fn flag(doc: &Doc, path: &str) -> R<bool> {
    match doc {
        Doc::Bool(b) => Ok(*b),
        _ => refuse(path, Kind::WrongType("a boolean")),
    }
}

fn need<'d>(entries: &'d [(String, Doc)], key: &'static str, path: &str) -> R<&'d Doc> {
    field(entries, key).map_or_else(|| refuse(path, Kind::Missing(key)), Ok)
}

fn only(entries: &[(String, Doc)], known: &[&str], path: &str) -> R<()> {
    match entries.iter().find(|(k, _)| !known.contains(&k.as_str())) {
        Some((k, _)) => refuse(&at(path, k), Kind::UnknownField(k.clone())),
        None => Ok(()),
    }
}

/// An RFC 1035 label: a lowercase letter, then lowercase letters, digits
/// and hyphens, not ending in a hyphen, at most 63 characters.
#[must_use]
pub fn label(t: &str) -> bool {
    let c: Vec<char> = t.chars().collect();
    !c.is_empty()
        && c.len() <= 63
        && c[0].is_ascii_lowercase()
        && c[c.len() - 1] != '-'
        && c.iter()
            .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || *x == '-')
}

/// The vertex-id segment of a property name.
#[must_use]
pub fn segment(name: &str) -> String {
    name.replace('~', "~0")
        .replace('.', "~1")
        .replace('[', "~2")
        .replace('{', "~3")
}

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
            c if (c as u32) < 0x20 => {
                let hex = b"0123456789abcdef";
                let n = c as usize;
                out.push_str("\\u00");
                out.push(char::from(hex[n >> 4]));
                out.push(char::from(hex[n & 15]));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The compact JSON text of a tree, entries in order.
#[must_use]
pub fn json(doc: &Doc) -> String {
    match doc {
        Doc::Null => "null".into(),
        Doc::Bool(b) => b.to_string(),
        Doc::Int(i) => i.to_string(),
        Doc::Str(t) => json_string(t),
        Doc::Seq(items) => format!("[{}]", items.iter().map(json).collect::<Vec<_>>().join(",")),
        Doc::Map(entries) => format!(
            "{{{}}}",
            entries
                .iter()
                .map(|(k, v)| format!("{}:{}", json_string(k), json(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

struct State {
    out: Reading,
}

impl State {
    fn vertex(&mut self, id: &str, kind: &str) {
        self.out.vertices.insert(id.to_owned(), kind.to_owned());
    }

    fn edge(&mut self, src: &str, kind: &str, tgt: &str, name: Option<&str>) {
        self.out.edges.entry(src.to_owned()).or_default().push((
            kind.to_owned(),
            tgt.to_owned(),
            name.map(str::to_owned),
        ));
    }

    fn keep(&mut self, vertex: &str, sort: &str, value: &Doc) {
        self.out
            .constraints
            .entry(vertex.to_owned())
            .or_default()
            .push((sort.to_owned(), json(value)));
    }
}

/// Read a CRD as the page states.
///
/// # Errors
///
/// The first [`Refusal`], in the page's order.
pub fn read(doc: &Doc) -> Result<Reading, Refusal> {
    let top = map(doc, "")?;
    only(
        top,
        &["apiVersion", "kind", "metadata", "spec", "status"],
        "",
    )?;
    if text(need(top, "apiVersion", "")?, "apiVersion")? != API_VERSION {
        return refuse("apiVersion", Kind::ApiVersion);
    }
    if text(need(top, "kind", "")?, "kind")? != "CustomResourceDefinition" {
        return refuse("kind", Kind::Kind);
    }
    let meta = map(need(top, "metadata", "")?, "metadata")?;
    only(meta, &METADATA, "metadata")?;
    let name = text(need(meta, "name", "metadata")?, "metadata.name")?.to_owned();
    let mut st = State {
        out: Reading::default(),
    };
    st.vertex(&name, "custom-resource");
    let rest: Vec<(String, Doc)> = meta.iter().filter(|(k, _)| k != "name").cloned().collect();
    if !rest.is_empty() {
        st.keep(&name, "metadata", &Doc::Map(rest));
    }
    let spec = map(need(top, "spec", "")?, "spec")?;
    only(
        spec,
        &[
            "group",
            "names",
            "scope",
            "versions",
            "conversion",
            "preserveUnknownFields",
        ],
        "spec",
    )?;
    let group_doc = need(spec, "group", "spec")?;
    let group = text(group_doc, "spec.group")?;
    if !group.contains('.') {
        return refuse("spec.group", Kind::Invalid);
    }
    st.keep(&name, "group", group_doc);
    let plural = names(&mut st, &name, need(spec, "names", "spec")?)?;
    let scope = need(spec, "scope", "spec")?;
    if !["Namespaced", "Cluster"].contains(&text(scope, "spec.scope")?) {
        return refuse("spec.scope", Kind::Invalid);
    }
    st.keep(&name, "scope", scope);
    versions(&mut st, &name, need(spec, "versions", "spec")?)?;
    if let Some(conversion) = field(spec, "conversion") {
        let c = map(conversion, "spec.conversion")?;
        only(c, &["strategy", "webhook"], "spec.conversion")?;
        if let Some(strategy) = field(c, "strategy") {
            match text(strategy, "spec.conversion.strategy")? {
                "None" => {}
                "Webhook" => {
                    return refuse(
                        "spec.conversion.strategy",
                        Kind::Unsupported("a conversion webhook"),
                    );
                }
                _ => return refuse("spec.conversion.strategy", Kind::Invalid),
            }
        }
        if field(c, "webhook").is_some() {
            return refuse(
                "spec.conversion.webhook",
                Kind::Unsupported("a conversion webhook"),
            );
        }
        st.keep(&name, "conversion", conversion);
    }
    if let Some(preserve) = field(spec, "preserveUnknownFields") {
        if flag(preserve, "spec.preserveUnknownFields")? {
            return refuse("spec.preserveUnknownFields", Kind::Invalid);
        }
        st.keep(&name, "preserveUnknownFields", preserve);
    }
    if name != format!("{plural}.{group}") {
        return refuse("metadata.name", Kind::Invalid);
    }
    if let Some(status) = field(top, "status") {
        map(status, "status")?;
        st.keep(&name, "status", status);
    }
    Ok(st.out)
}

fn names(st: &mut State, crd: &str, doc: &Doc) -> R<String> {
    let p = "spec.names";
    let n = map(doc, p)?;
    only(
        n,
        &[
            "plural",
            "singular",
            "kind",
            "listKind",
            "shortNames",
            "categories",
        ],
        p,
    )?;
    let plural_doc = need(n, "plural", p)?;
    let plural = text(plural_doc, "spec.names.plural")?;
    if !label(plural) {
        return refuse("spec.names.plural", Kind::Invalid);
    }
    st.keep(crd, "plural", plural_doc);
    if let Some(singular) = field(n, "singular") {
        if !label(text(singular, "spec.names.singular")?) {
            return refuse("spec.names.singular", Kind::Invalid);
        }
        st.keep(crd, "singular", singular);
    }
    let kind = need(n, "kind", p)?;
    if text(kind, "spec.names.kind")?.is_empty() {
        return refuse("spec.names.kind", Kind::Invalid);
    }
    st.keep(crd, "kind", kind);
    if let Some(list_kind) = field(n, "listKind") {
        text(list_kind, "spec.names.listKind")?;
        st.keep(crd, "listKind", list_kind);
    }
    for key in ["shortNames", "categories"] {
        if let Some(list) = field(n, key) {
            let lp = at(p, key);
            for (i, item) in seq(list, &lp)?.iter().enumerate() {
                text(item, &format!("{lp}[{i}]"))?;
            }
            st.keep(crd, key, list);
        }
    }
    Ok(plural.to_owned())
}

fn versions(st: &mut State, crd: &str, doc: &Doc) -> R<()> {
    let p = "spec.versions";
    let items = seq(doc, p)?;
    if items.is_empty() {
        return refuse(p, Kind::Invalid);
    }
    let mut seen = BTreeSet::new();
    let mut storage = 0;
    for (i, item) in items.iter().enumerate() {
        let vp = format!("{p}[{i}]");
        let v = map(item, &vp)?;
        only(
            v,
            &[
                "name",
                "served",
                "storage",
                "deprecated",
                "deprecationWarning",
                "schema",
                "subresources",
                "additionalPrinterColumns",
                "selectableFields",
            ],
            &vp,
        )?;
        let np = at(&vp, "name");
        let name = text(need(v, "name", &vp)?, &np)?;
        if !label(name) {
            return refuse(&np, Kind::Invalid);
        }
        if !seen.insert(name.to_owned()) {
            return refuse(&np, Kind::DuplicateVersion(name.to_owned()));
        }
        let id = format!("{crd}/{name}");
        st.vertex(&id, "version");
        st.edge(crd, "version", &id, Some(name));
        for key in ["served", "storage"] {
            let d = need(v, key, &vp)?;
            if flag(d, &at(&vp, key))? && key == "storage" {
                storage += 1;
            }
            st.keep(&id, key, d);
        }
        if let Some(d) = field(v, "deprecated") {
            flag(d, &at(&vp, "deprecated"))?;
            st.keep(&id, "deprecated", d);
        }
        if let Some(d) = field(v, "deprecationWarning") {
            text(d, &at(&vp, "deprecationWarning"))?;
            st.keep(&id, "deprecationWarning", d);
        }
        let sp = at(&vp, "schema");
        let schema = map(need(v, "schema", &vp)?, &sp)?;
        only(schema, &["openAPIV3Schema"], &sp)?;
        let rp = at(&sp, "openAPIV3Schema");
        let root = format!("{id}#");
        if node(st, need(schema, "openAPIV3Schema", &sp)?, &rp, &root, None)? != "object" {
            return refuse(&rp, Kind::Invalid);
        }
        st.edge(&id, "schema", &root, None);
        if let Some(d) = field(v, "subresources") {
            map(d, &at(&vp, "subresources"))?;
            st.keep(&id, "subresources", d);
        }
        for key in ["additionalPrinterColumns", "selectableFields"] {
            if let Some(d) = field(v, key) {
                let lp = at(&vp, key);
                for (j, x) in seq(d, &lp)?.iter().enumerate() {
                    map(x, &format!("{lp}[{j}]"))?;
                }
                st.keep(&id, key, d);
            }
        }
    }
    if storage != 1 {
        return refuse(p, Kind::StorageVersions(storage));
    }
    Ok(())
}

/// Check a kept keyword's value against the page's table.
fn keyword(key: &str, value: &Doc, p: &str, kind: &str) -> R<()> {
    match key {
        "description" | "title" | "format" | "pattern" => {
            text(value, p)?;
        }
        "nullable"
        | "exclusiveMaximum"
        | "exclusiveMinimum"
        | "x-kubernetes-preserve-unknown-fields"
        | "x-kubernetes-embedded-resource"
        | "x-kubernetes-int-or-string" => {
            flag(value, p)?;
        }
        "uniqueItems" => {
            if flag(value, p)? {
                return refuse(p, Kind::Forbidden("uniqueItems: true"));
            }
        }
        "maximum" | "minimum" | "multipleOf" => {
            if !matches!(value, Doc::Int(_)) {
                return refuse(p, Kind::WrongType("a number"));
            }
        }
        "maxLength" | "minLength" | "maxItems" | "minItems" | "maxProperties" | "minProperties" => {
            if !matches!(value, Doc::Int(n) if *n >= 0) {
                return refuse(p, Kind::WrongType("a non-negative integer"));
            }
        }
        "enum" | "allOf" | "anyOf" | "oneOf" | "x-kubernetes-validations" => {
            seq(value, p)?;
        }
        "required" | "x-kubernetes-list-map-keys" => {
            for (i, x) in seq(value, p)?.iter().enumerate() {
                text(x, &format!("{p}[{i}]"))?;
            }
        }
        "not" | "externalDocs" => {
            map(value, p)?;
        }
        "x-kubernetes-list-type" => {
            let allowed = ["atomic", "set", "map"].contains(&text(value, p)?) && kind == "array";
            if !allowed {
                return refuse(p, Kind::Invalid);
            }
        }
        "x-kubernetes-map-type" => {
            let allowed = ["atomic", "granular"].contains(&text(value, p)?) && kind == "object";
            if !allowed {
                return refuse(p, Kind::Invalid);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Read schema node `doc` at path `p` as vertex `id`, reached by `parent`.
fn node(
    st: &mut State,
    doc: &Doc,
    p: &str,
    id: &str,
    parent: Option<(&str, &str, Option<&str>)>,
) -> R<&'static str> {
    let e = map(doc, p)?;
    for (k, _) in e {
        if let Some(f) = FORBIDDEN.iter().find(|f| *f == k) {
            return refuse(&at(p, k), Kind::Forbidden(f));
        }
        let shaping = ["type", "properties", "items", "additionalProperties"];
        if !shaping.contains(&k.as_str()) && !KEPT.contains(&k.as_str()) {
            return refuse(&at(p, k), Kind::UnknownField(k.clone()));
        }
    }
    let is_set =
        |key: &str| -> R<bool> { field(e, key).map_or(Ok(false), |d| flag(d, &at(p, key))) };
    let int_or_string = is_set("x-kubernetes-int-or-string")?;
    let preserve = is_set("x-kubernetes-preserve-unknown-fields")?;
    let kind: &'static str = match field(e, "type") {
        Some(t) => {
            let tp = at(p, "type");
            let name = text(t, &tp)?;
            let Some(k) = TYPES.iter().find(|x| **x == name) else {
                return refuse(&tp, Kind::Invalid);
            };
            k
        }
        None if int_or_string => "int-or-string",
        None if preserve => "any",
        None => return refuse(p, Kind::Missing("type")),
    };
    st.vertex(id, kind);
    if let Some((src, edge, name)) = parent {
        st.edge(src, edge, id, name);
    }
    for (k, v) in e {
        if !["type", "properties", "items", "additionalProperties"].contains(&k.as_str()) {
            keyword(k, v, &at(p, k), kind)?;
            st.keep(id, k, v);
        }
    }
    let mut props = Vec::new();
    if let Some(d) = field(e, "properties") {
        let pp = at(p, "properties");
        if kind != "object" {
            return refuse(&pp, Kind::Invalid);
        }
        for (name, sub) in map(d, &pp)? {
            let child = format!("{id}.{}", segment(name));
            node(
                st,
                sub,
                &at(&pp, name),
                &child,
                Some((id, "prop", Some(name))),
            )?;
            props.push(name.clone());
        }
    }
    if let Some(d) = field(e, "additionalProperties") {
        let ap = at(p, "additionalProperties");
        if kind != "object" || field(e, "properties").is_some() {
            return refuse(&ap, Kind::Invalid);
        }
        if matches!(d, Doc::Bool(_)) {
            return refuse(&ap, Kind::Unsupported("a boolean additionalProperties"));
        }
        node(st, d, &ap, &format!("{id}{{}}"), Some((id, "values", None)))?;
    }
    match field(e, "items") {
        Some(d) => {
            let ip = at(p, "items");
            if kind != "array" || matches!(d, Doc::Seq(_)) {
                return refuse(&ip, Kind::Invalid);
            }
            node(st, d, &ip, &format!("{id}[]"), Some((id, "items", None)))?;
        }
        None if kind == "array" => return refuse(p, Kind::Missing("items")),
        None => {}
    }
    list_map(st, e, p, id)?;
    if let Some(d) = field(e, "required") {
        let rp = at(p, "required");
        for (i, x) in seq(d, &rp)?.iter().enumerate() {
            let Doc::Str(name) = x else { continue };
            if props.contains(name) {
                st.out
                    .required
                    .entry(id.to_owned())
                    .or_default()
                    .insert(name.clone());
            } else {
                st.out.unresolved.push((format!("{rp}[{i}]"), name.clone()));
            }
        }
    }
    Ok(kind)
}

fn list_map(st: &mut State, e: &[(String, Doc)], p: &str, id: &str) -> R<()> {
    let list_type = match field(e, "x-kubernetes-list-type") {
        Some(Doc::Str(t)) => Some(t.as_str()),
        _ => None,
    };
    let kp = at(p, "x-kubernetes-list-map-keys");
    match (list_type, field(e, "x-kubernetes-list-map-keys")) {
        (Some("map"), None) => refuse(
            &at(p, "x-kubernetes-list-type"),
            Kind::Missing("x-kubernetes-list-map-keys"),
        ),
        (Some("map"), Some(Doc::Seq(keys))) => {
            if keys.is_empty() {
                return refuse(&kp, Kind::Invalid);
            }
            let item_props: &[(String, Doc)] = match field(e, "items") {
                Some(Doc::Map(ie)) => match field(ie, "properties") {
                    Some(Doc::Map(pe)) => pe,
                    _ => &[],
                },
                _ => &[],
            };
            let mut names = Vec::new();
            for (i, k) in keys.iter().enumerate() {
                let Doc::Str(name) = k else { continue };
                let scalar = match field(item_props, name) {
                    Some(Doc::Map(pe)) => matches!(
                        field(pe, "type"),
                        Some(Doc::Str(t)) if ["string", "integer", "number", "boolean"].contains(&t.as_str())
                    ),
                    _ => false,
                };
                if !scalar {
                    return refuse(&format!("{kp}[{i}]"), Kind::ListMapKey(name.clone()));
                }
                names.push(name.clone());
            }
            st.out
                .keys
                .insert(format!("{id}[]"), (id.to_owned(), names));
            Ok(())
        }
        (_, Some(_)) => refuse(&kp, Kind::Invalid),
        _ => Ok(()),
    }
}

/// The canonical CRD of an accepted document, as the page defines it: the
/// compact JSON of the document with its fields reordered, followed by a
/// line feed.
#[must_use]
pub fn canonical_text(doc: &Doc) -> String {
    let Doc::Map(top) = doc else {
        return String::new();
    };
    let get = |e: &[(String, Doc)], k: &str| field(e, k).cloned();
    let pick = |e: &[(String, Doc)], keys: &[&str]| -> Vec<(String, String)> {
        keys.iter()
            .filter_map(|k| get(e, k).map(|v| ((*k).to_owned(), canonical_value(k, &v))))
            .collect()
    };
    let object = |fields: &[(String, String)]| {
        format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(k, v)| format!("{}:{v}", json_string(k)))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let mut fields = vec![
        ("apiVersion".to_owned(), json_string(API_VERSION)),
        ("kind".to_owned(), json_string("CustomResourceDefinition")),
    ];
    if let Some(Doc::Map(meta)) = get(top, "metadata") {
        let mut ordered: Vec<(String, String)> = pick(&meta, &["name"]);
        ordered.extend(
            meta.iter()
                .filter(|(k, _)| k != "name")
                .map(|(k, v)| (k.clone(), json(v))),
        );
        fields.push(("metadata".into(), object(&ordered)));
    }
    if let Some(Doc::Map(spec)) = get(top, "spec") {
        let mut sf = pick(&spec, &["group"]);
        if let Some(Doc::Map(names)) = get(&spec, "names") {
            sf.push((
                "names".into(),
                object(&pick(
                    &names,
                    &[
                        "plural",
                        "singular",
                        "kind",
                        "listKind",
                        "shortNames",
                        "categories",
                    ],
                )),
            ));
        }
        sf.extend(pick(&spec, &["scope"]));
        if let Some(Doc::Seq(vs)) = get(&spec, "versions") {
            let texts: Vec<String> = vs
                .iter()
                .map(|v| {
                    let Doc::Map(v) = v else { return String::new() };
                    let mut vf = pick(
                        v,
                        &[
                            "name",
                            "served",
                            "storage",
                            "deprecated",
                            "deprecationWarning",
                        ],
                    );
                    if let Some(Doc::Map(schema)) = get(v, "schema")
                        && let Some(root) = get(&schema, "openAPIV3Schema")
                    {
                        vf.push((
                            "schema".into(),
                            object(&[("openAPIV3Schema".into(), canonical_node(&root))]),
                        ));
                    }
                    vf.extend(pick(
                        v,
                        &[
                            "subresources",
                            "additionalPrinterColumns",
                            "selectableFields",
                        ],
                    ));
                    object(&vf)
                })
                .collect();
            sf.push(("versions".into(), format!("[{}]", texts.join(","))));
        }
        sf.extend(pick(&spec, &["conversion", "preserveUnknownFields"]));
        fields.push(("spec".into(), object(&sf)));
    }
    fields.extend(pick(top, &["status"]));
    format!("{}\n", object(&fields))
}

fn canonical_value(_key: &str, v: &Doc) -> String {
    json(v)
}

/// The canonical text of a schema node: `type`, the kept keywords in order,
/// then `properties`, `additionalProperties` and `items`.
fn canonical_node(doc: &Doc) -> String {
    let Doc::Map(e) = doc else { return json(doc) };
    let mut parts = Vec::new();
    if let Some(t) = field(e, "type") {
        parts.push(format!("\"type\":{}", json(t)));
    }
    for (k, v) in e {
        if !["type", "properties", "items", "additionalProperties"].contains(&k.as_str()) {
            parts.push(format!("{}:{}", json_string(k), json(v)));
        }
    }
    if let Some(Doc::Map(props)) = field(e, "properties")
        && !props.is_empty()
    {
        let inner: Vec<String> = props
            .iter()
            .map(|(k, v)| format!("{}:{}", json_string(k), canonical_node(v)))
            .collect();
        parts.push(format!("\"properties\":{{{}}}", inner.join(",")));
    }
    if let Some(d) = field(e, "additionalProperties") {
        parts.push(format!("\"additionalProperties\":{}", canonical_node(d)));
    }
    if let Some(d) = field(e, "items") {
        parts.push(format!("\"items\":{}", canonical_node(d)));
    }
    format!("{{{}}}", parts.join(","))
}

/// The block YAML text of a tree: two-space indentation, every string and
/// every key written as a JSON string (which YAML 1.2 reads as the same
/// string), an empty mapping as `{}` and an empty sequence as `[]`.
#[must_use]
pub fn render_yaml(doc: &Doc) -> String {
    let mut out = String::new();
    block(doc, 0, &mut out);
    out
}

fn inline(doc: &Doc) -> Option<String> {
    match doc {
        Doc::Map(e) if e.is_empty() => Some("{}".into()),
        Doc::Seq(i) if i.is_empty() => Some("[]".into()),
        Doc::Map(_) | Doc::Seq(_) => None,
        scalar => Some(json(scalar)),
    }
}

fn block(doc: &Doc, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    match doc {
        Doc::Map(e) if !e.is_empty() => {
            for (k, v) in e {
                if let Some(t) = inline(v) {
                    let _ = writeln!(out, "{pad}{}: {t}", json_string(k));
                } else {
                    let _ = writeln!(out, "{pad}{}:", json_string(k));
                    block(v, indent + 2, out);
                }
            }
        }
        Doc::Seq(items) if !items.is_empty() => {
            for item in items {
                if let Some(t) = inline(item) {
                    let _ = writeln!(out, "{pad}- {t}");
                } else {
                    let mut inner = String::new();
                    block(item, indent + 2, &mut inner);
                    out.push_str(&pad);
                    out.push_str("- ");
                    out.push_str(&inner[indent + 2..]);
                }
            }
        }
        other => {
            out.push_str(&pad);
            out.push_str(&inline(other).unwrap_or_default());
            out.push('\n');
        }
    }
}
