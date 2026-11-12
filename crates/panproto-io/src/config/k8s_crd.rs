//! The Kubernetes `CustomResourceDefinition` reader and its canonical writer.
//!
//! [`read_crd`] reads one `apiextensions.k8s.io/v1` CRD, as Kubernetes 1.37
//! defines it, through the shared YAML route ([`super::tree::read_yaml`])
//! into a schema of the `k8s-crd` protocol, and refuses with a [`CrdError`]
//! naming the field at fault (its path and byte offset) every document the
//! reader does not carry exactly. [`write_crd`] writes such a schema back as
//! the canonical CRD: JSON, which YAML 1.2 reads as the same document.
//!
//! The vertex ids are built from the CRD's name `<crd>`: the resource is
//! `<crd>`, a version `v` is `<crd>/v`, the root schema of that version is
//! `<crd>/v#`, and below a schema node `n` a property `p` is `n.p`, the
//! items schema `n[]` and the `additionalProperties` schema `n{}`, where `p`
//! is written with `~` as `~0`, `.` as `~1`, `[` as `~2` and `{` as `~3`.

use std::collections::BTreeSet;
use std::fmt;

use panproto_gat::Name;
use panproto_schema::{Edge, Key, KeyPart, KeyRole, KeyScope, Schema, SchemaBuilder};

use super::tree::{Content, JsonTextError, Node, json_string, read_yaml};

/// The API version the reader reads.
pub const API_VERSION: &str = "apiextensions.k8s.io/v1";

/// The Kubernetes release whose CRD definition the reader follows.
pub const KUBERNETES_VERSION: &str = "1.37";

/// The `type` values of a structural schema node.
const TYPES: &[&str] = &["object", "array", "string", "integer", "number", "boolean"];

/// The `JSONSchemaProps` fields Kubernetes refuses in a CRD.
const FORBIDDEN: &[&str] = &[
    "$ref",
    "$schema",
    "id",
    "definitions",
    "dependencies",
    "additionalItems",
    "patternProperties",
];

/// The `ObjectMeta` fields a CRD's `metadata` may hold.
const METADATA_FIELDS: &[&str] = &[
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

/// A refusal of the CRD reader: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrdError {
    /// The path of the field at fault, keys joined by `.` and sequence
    /// indices as `[i]`; empty for the document itself.
    pub path: String,
    /// The byte offset at which the field's value starts.
    pub offset: usize,
    /// What is wrong.
    pub kind: CrdErrorKind,
}

impl fmt::Display for CrdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if self.path.is_empty() {
            "the document"
        } else {
            &self.path
        };
        write!(f, "{path} (byte {}): {}", self.offset, self.kind)
    }
}

impl std::error::Error for CrdError {}

/// What a [`CrdError`] refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CrdErrorKind {
    /// The shared YAML route refuses the document.
    #[error("the YAML route refuses the document: {0}")]
    Yaml(String),
    /// `apiVersion` names another API version (`v1beta1` among them).
    #[error("apiVersion is {0:?}, not apiextensions.k8s.io/v1")]
    ApiVersion(String),
    /// `kind` is not `CustomResourceDefinition`.
    #[error("kind is {0:?}, not CustomResourceDefinition")]
    Kind(String),
    /// A required field is absent.
    #[error("the required field {0} is missing")]
    Missing(&'static str),
    /// A field holds a value of another kind.
    #[error("expected {0}")]
    WrongType(&'static str),
    /// A field breaks a rule of the CRD definition, which the message states.
    #[error("{0}")]
    Invalid(&'static str),
    /// A field the CRD definition does not declare.
    #[error("unknown field {0:?}")]
    UnknownField(String),
    /// A schema field Kubernetes refuses in a CRD.
    #[error("{0} is forbidden in a CustomResourceDefinition")]
    Forbidden(&'static str),
    /// A feature the reader does not carry.
    #[error("{0} is not supported")]
    Unsupported(&'static str),
    /// A version name given twice.
    #[error("version {0:?} is declared twice")]
    DuplicateVersion(String),
    /// Not exactly one version is the storage version.
    #[error("{0} versions are storage versions; exactly one must be")]
    StorageVersions(usize),
    /// The schema the document describes cannot be built.
    #[error("the schema cannot be built: {0}")]
    Schema(String),
    /// An `x-kubernetes-list-map-keys` entry names no scalar property of
    /// the list's items.
    #[error("list map key {0:?} names no scalar property of the items")]
    ListMapKey(String),
}

/// A `required` entry that names no property of its schema node.
///
/// Kubernetes accepts such an entry (a node that keeps unknown fields may
/// require one it does not declare), so the reader keeps it in the node's
/// `required` constraint and reports it here rather than refusing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnresolvedRequired {
    /// The path of the `required` entry.
    pub path: String,
    /// The byte offset of the entry.
    pub offset: usize,
    /// The name it gives.
    pub name: String,
}

/// What [`read_crd`] reads: the schema, and the `required` entries that
/// resolve to no property, in document order.
#[derive(Clone, Debug)]
pub struct CrdReading {
    /// The schema of the `k8s-crd` protocol.
    pub schema: Schema,
    /// The unresolved `required` entries.
    pub unresolved: Vec<UnresolvedRequired>,
}

type R<T> = Result<T, CrdError>;

const fn error(path: String, offset: usize, kind: CrdErrorKind) -> CrdError {
    CrdError { path, offset, kind }
}

fn child(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn get<'n>(entries: &'n [(String, Node)], key: &str) -> Option<&'n Node> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The vertex-id segment of property `name`.
#[must_use]
pub fn escape_segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '~' => out.push_str("~0"),
            '.' => out.push_str("~1"),
            '[' => out.push_str("~2"),
            '{' => out.push_str("~3"),
            c => out.push(c),
        }
    }
    out
}

/// Whether `text` is an RFC 1035 label, as Kubernetes checks a version name
/// and a resource's plural and singular names.
fn dns_label(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// The reader's state: the schema built so far and the unresolved entries.
struct Reader {
    builder: Option<SchemaBuilder>,
    unresolved: Vec<UnresolvedRequired>,
}

impl Reader {
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
        let built = step(builder)
            .map_err(|e| error(path.to_owned(), offset, CrdErrorKind::Schema(e.to_string())))?;
        self.builder = Some(built);
        Ok(())
    }

    fn constraint(&mut self, vertex: &str, sort: &str, node: &Node, path: &str) -> R<()> {
        let text = json(node, path)?;
        self.with(|b| b.constraint(vertex, sort, &text));
        Ok(())
    }
}

/// `node`'s canonical JSON text, or the refusal of the scalar JSON cannot
/// write.
fn json(node: &Node, path: &str) -> R<String> {
    node.json_text().map_err(|e: JsonTextError| {
        error(
            path.to_owned(),
            node.offset,
            CrdErrorKind::Unsupported(e.feature()),
        )
    })
}

/// The node's mapping entries, or a `WrongType` refusal.
fn mapping<'n>(node: &'n Node, path: &str) -> R<&'n [(String, Node)]> {
    node.mapping().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            CrdErrorKind::WrongType("a mapping"),
        )
    })
}

fn sequence<'n>(node: &'n Node, path: &str) -> R<&'n [Node]> {
    node.sequence().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            CrdErrorKind::WrongType("a sequence"),
        )
    })
}

fn string<'n>(node: &'n Node, path: &str) -> R<&'n str> {
    node.string().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            CrdErrorKind::WrongType("a string"),
        )
    })
}

fn boolean(node: &Node, path: &str) -> R<bool> {
    node.boolean().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            CrdErrorKind::WrongType("a boolean"),
        )
    })
}

/// Refuse the first key of `entries`, in source order, that `known` lacks.
fn known_keys(entries: &[(String, Node)], known: &[&str], path: &str) -> R<()> {
    for (key, value) in entries {
        if !known.contains(&key.as_str()) {
            return Err(error(
                child(path, key),
                value.offset,
                CrdErrorKind::UnknownField(key.clone()),
            ));
        }
    }
    Ok(())
}

/// The field `key` of a mapping, or a `Missing` refusal at the mapping.
fn required<'n>(
    entries: &'n [(String, Node)],
    key: &'static str,
    path: &str,
    offset: usize,
) -> R<&'n Node> {
    get(entries, key).ok_or_else(|| error(path.to_owned(), offset, CrdErrorKind::Missing(key)))
}

/// Read `source`, one `apiextensions.k8s.io/v1` CRD in YAML or JSON, into a
/// `k8s-crd` schema.
///
/// The fields are checked in this order, and the first refusal is
/// returned: at every mapping, each key in source order against the fields
/// the CRD definition declares there; then `apiVersion`, `kind`,
/// `metadata`, then the `spec` fields `group`, `names`, `scope`,
/// `versions` (each version's `name`, its uniqueness, `served`, `storage`,
/// `deprecated`, `deprecationWarning`, `schema`, `subresources`,
/// `additionalPrinterColumns` and `selectableFields`, then the count of
/// storage versions), `conversion` and `preserveUnknownFields`; then the
/// CRD's name against `<plural>.<group>`; and last `status`.
///
/// # Errors
///
/// Returns a [`CrdError`] naming the field at fault.
pub fn read_crd(source: &[u8]) -> Result<CrdReading, CrdError> {
    let document = read_yaml(source)
        .map_err(|e| error(String::new(), 0, CrdErrorKind::Yaml(e.to_string())))?;
    read_document(&document)
}

/// Read a document tree into a `k8s-crd` schema, as [`read_crd`] does.
///
/// # Errors
///
/// Returns a [`CrdError`] naming the field at fault.
pub fn read_document(document: &Node) -> Result<CrdReading, CrdError> {
    let top = mapping(document, "")?;
    known_keys(
        top,
        &["apiVersion", "kind", "metadata", "spec", "status"],
        "",
    )?;
    let api_version = required(top, "apiVersion", "", document.offset)?;
    let text = string(api_version, "apiVersion")?;
    if text != API_VERSION {
        return Err(error(
            "apiVersion".into(),
            api_version.offset,
            CrdErrorKind::ApiVersion(text.to_owned()),
        ));
    }
    let kind = required(top, "kind", "", document.offset)?;
    let text = string(kind, "kind")?;
    if text != "CustomResourceDefinition" {
        return Err(error(
            "kind".into(),
            kind.offset,
            CrdErrorKind::Kind(text.to_owned()),
        ));
    }
    let metadata = required(top, "metadata", "", document.offset)?;
    let meta = mapping(metadata, "metadata")?;
    known_keys(meta, METADATA_FIELDS, "metadata")?;
    let name_node = required(meta, "name", "metadata", metadata.offset)?;
    let name = string(name_node, "metadata.name")?.to_owned();

    let proto = panproto_protocols::config::k8s_crd::protocol();
    let mut reader = Reader {
        builder: Some(SchemaBuilder::new(&proto)),
        unresolved: Vec::new(),
    };
    reader.try_with("metadata.name", name_node.offset, |b| {
        b.vertex(&name, "custom-resource", None)
    })?;
    let rest: Vec<(String, Node)> = meta.iter().filter(|(k, _)| k != "name").cloned().collect();
    if !rest.is_empty() {
        let node = Node {
            content: Content::Mapping(rest),
            offset: metadata.offset,
        };
        reader.constraint(&name, "metadata", &node, "metadata")?;
    }

    let spec = required(top, "spec", "", document.offset)?;
    read_spec(&mut reader, &name, name_node, spec)?;
    if let Some(status) = get(top, "status") {
        mapping(status, "status")?;
        reader.constraint(&name, "status", status, "status")?;
    }
    let schema = reader.builder.take().map_or_else(
        || {
            Err(error(
                String::new(),
                0,
                CrdErrorKind::Schema("no builder".into()),
            ))
        },
        |b| {
            b.build()
                .map_err(|e| error(String::new(), 0, CrdErrorKind::Schema(e.to_string())))
        },
    )?;
    Ok(CrdReading {
        schema,
        unresolved: reader.unresolved,
    })
}

fn read_spec(reader: &mut Reader, crd: &str, name_node: &Node, spec: &Node) -> R<()> {
    let entries = mapping(spec, "spec")?;
    known_keys(
        entries,
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
    let group_node = required(entries, "group", "spec", spec.offset)?;
    let group = string(group_node, "spec.group")?;
    if !group.contains('.') {
        return Err(error(
            "spec.group".into(),
            group_node.offset,
            CrdErrorKind::Invalid("the group must be a domain name with at least one dot"),
        ));
    }
    reader.constraint(crd, "group", group_node, "spec.group")?;

    let spec_names = required(entries, "names", "spec", spec.offset)?;
    let plural = read_names(reader, crd, spec_names)?;

    let scope = required(entries, "scope", "spec", spec.offset)?;
    if !matches!(string(scope, "spec.scope")?, "Namespaced" | "Cluster") {
        return Err(error(
            "spec.scope".into(),
            scope.offset,
            CrdErrorKind::Invalid("the scope must be Namespaced or Cluster"),
        ));
    }
    reader.constraint(crd, "scope", scope, "spec.scope")?;

    let versions = required(entries, "versions", "spec", spec.offset)?;
    read_versions(reader, crd, versions)?;

    if let Some(conversion) = get(entries, "conversion") {
        let fields = mapping(conversion, "spec.conversion")?;
        known_keys(fields, &["strategy", "webhook"], "spec.conversion")?;
        if let Some(strategy) = get(fields, "strategy") {
            match string(strategy, "spec.conversion.strategy")? {
                "None" => {}
                "Webhook" => {
                    return Err(error(
                        "spec.conversion.strategy".into(),
                        strategy.offset,
                        CrdErrorKind::Unsupported("a conversion webhook"),
                    ));
                }
                _ => {
                    return Err(error(
                        "spec.conversion.strategy".into(),
                        strategy.offset,
                        CrdErrorKind::Invalid("the conversion strategy must be None or Webhook"),
                    ));
                }
            }
        }
        if let Some(webhook) = get(fields, "webhook") {
            return Err(error(
                "spec.conversion.webhook".into(),
                webhook.offset,
                CrdErrorKind::Unsupported("a conversion webhook"),
            ));
        }
        reader.constraint(crd, "conversion", conversion, "spec.conversion")?;
    }
    if let Some(preserve) = get(entries, "preserveUnknownFields") {
        if boolean(preserve, "spec.preserveUnknownFields")? {
            return Err(error(
                "spec.preserveUnknownFields".into(),
                preserve.offset,
                CrdErrorKind::Invalid(
                    "preserveUnknownFields must be false in apiextensions.k8s.io/v1",
                ),
            ));
        }
        reader.constraint(
            crd,
            "preserveUnknownFields",
            preserve,
            "spec.preserveUnknownFields",
        )?;
    }
    if crd != format!("{plural}.{group}") {
        return Err(error(
            "metadata.name".into(),
            name_node.offset,
            CrdErrorKind::Invalid("the name must be <plural>.<group>"),
        ));
    }
    Ok(())
}

/// Read `spec.names`, returning the plural name.
fn read_names(reader: &mut Reader, crd: &str, names: &Node) -> R<String> {
    let path = "spec.names";
    let entries = mapping(names, path)?;
    known_keys(
        entries,
        &[
            "plural",
            "singular",
            "kind",
            "listKind",
            "shortNames",
            "categories",
        ],
        path,
    )?;
    let plural_node = required(entries, "plural", path, names.offset)?;
    let plural = string(plural_node, "spec.names.plural")?.to_owned();
    let label = |node: &Node, at: &str, text: &str| {
        if dns_label(text) {
            Ok(())
        } else {
            Err(error(
                at.to_owned(),
                node.offset,
                CrdErrorKind::Invalid("the name must be a lowercase RFC 1035 label"),
            ))
        }
    };
    label(plural_node, "spec.names.plural", &plural)?;
    reader.constraint(crd, "plural", plural_node, "spec.names.plural")?;
    if let Some(singular) = get(entries, "singular") {
        let text = string(singular, "spec.names.singular")?;
        label(singular, "spec.names.singular", text)?;
        reader.constraint(crd, "singular", singular, "spec.names.singular")?;
    }
    let kind = required(entries, "kind", path, names.offset)?;
    if string(kind, "spec.names.kind")?.is_empty() {
        return Err(error(
            "spec.names.kind".into(),
            kind.offset,
            CrdErrorKind::Invalid("the kind must not be empty"),
        ));
    }
    reader.constraint(crd, "kind", kind, "spec.names.kind")?;
    if let Some(list_kind) = get(entries, "listKind") {
        string(list_kind, "spec.names.listKind")?;
        reader.constraint(crd, "listKind", list_kind, "spec.names.listKind")?;
    }
    for key in ["shortNames", "categories"] {
        if let Some(list) = get(entries, key) {
            let at = child(path, key);
            for (i, item) in sequence(list, &at)?.iter().enumerate() {
                string(item, &format!("{at}[{i}]"))?;
            }
            reader.constraint(crd, key, list, &at)?;
        }
    }
    Ok(plural)
}

fn read_versions(reader: &mut Reader, crd: &str, versions: &Node) -> R<()> {
    let path = "spec.versions";
    let items = sequence(versions, path)?;
    if items.is_empty() {
        return Err(error(
            path.into(),
            versions.offset,
            CrdErrorKind::Invalid("there must be at least one version"),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut storage = 0;
    for (i, version) in items.iter().enumerate() {
        if read_version(reader, crd, version, i, &mut seen)? {
            storage += 1;
        }
    }
    if storage != 1 {
        return Err(error(
            path.into(),
            versions.offset,
            CrdErrorKind::StorageVersions(storage),
        ));
    }
    Ok(())
}

/// The fields a schema node may hold: the kept keywords and the four that
/// shape the schema.
fn schema_field(key: &str) -> bool {
    matches!(
        key,
        "type" | "properties" | "items" | "additionalProperties"
    ) || panproto_protocols::config::k8s_crd::SCHEMA_CONSTRAINTS.contains(&key)
}

/// Read the schema node `node` at `path` into vertex `id`, returning its
/// vertex kind. `parent` is the edge that reaches it, added after the
/// vertex.
fn read_schema(
    reader: &mut Reader,
    node: &Node,
    path: &str,
    id: &str,
    parent: Option<(&str, &str, Option<&str>)>,
) -> R<&'static str> {
    let entries = mapping(node, path)?;
    for (key, value) in entries {
        if let Some(forbidden) = FORBIDDEN.iter().find(|f| **f == key) {
            return Err(error(
                child(path, key),
                value.offset,
                CrdErrorKind::Forbidden(forbidden),
            ));
        }
        if !schema_field(key) {
            return Err(error(
                child(path, key),
                value.offset,
                CrdErrorKind::UnknownField(key.clone()),
            ));
        }
    }
    let flag = |key: &str| -> R<bool> {
        get(entries, key).map_or(Ok(false), |v| boolean(v, &child(path, key)))
    };
    let int_or_string = flag("x-kubernetes-int-or-string")?;
    let preserve = flag("x-kubernetes-preserve-unknown-fields")?;
    let kind: &'static str = match get(entries, "type") {
        Some(ty) => {
            let at = child(path, "type");
            let text = string(ty, &at)?;
            let Some(kind) = TYPES.iter().find(|t| **t == text) else {
                return Err(error(
                    at,
                    ty.offset,
                    CrdErrorKind::Invalid(
                        "the type must be object, array, string, integer, number or boolean",
                    ),
                ));
            };
            kind
        }
        None if int_or_string => "int-or-string",
        None if preserve => "any",
        None => {
            return Err(error(
                path.to_owned(),
                node.offset,
                CrdErrorKind::Missing("type"),
            ));
        }
    };
    reader.try_with(path, node.offset, |b| {
        let b = b.vertex(id, kind, None)?;
        match parent {
            Some((src, edge, name)) => b.edge(src, id, edge, name),
            None => Ok(b),
        }
    })?;
    for (key, value) in entries {
        if matches!(
            key.as_str(),
            "type" | "properties" | "items" | "additionalProperties"
        ) {
            continue;
        }
        let at = child(path, key);
        check_keyword(key, value, &at, kind)?;
        reader.constraint(id, key, value, &at)?;
    }
    let (properties, item_properties) = read_children(reader, entries, node, path, id, kind)?;
    read_list_keys(reader, entries, path, id, item_properties)?;
    read_required(reader, entries, path, id, &properties);
    Ok(kind)
}

/// A schema node's properties with their vertex kinds, and its items
/// vertex id with the items node.
type Children<'n> = (Vec<(String, &'static str)>, Option<(String, &'n Node)>);

/// Read the `properties`, `additionalProperties` and `items` of schema
/// node `id`, returning its properties with their kinds and its items.
fn read_children<'n>(
    reader: &mut Reader,
    entries: &'n [(String, Node)],
    node: &Node,
    path: &str,
    id: &str,
    kind: &str,
) -> R<Children<'n>> {
    let mut properties = Vec::new();
    if let Some(props) = get(entries, "properties") {
        let at = child(path, "properties");
        if kind != "object" {
            return Err(error(
                at,
                props.offset,
                CrdErrorKind::Invalid("properties need type object"),
            ));
        }
        for (name, schema) in mapping(props, &at)? {
            let prop_id = format!("{id}.{}", escape_segment(name));
            let prop_kind = read_schema(
                reader,
                schema,
                &child(&at, name),
                &prop_id,
                Some((id, "prop", Some(name))),
            )?;
            properties.push((name.clone(), prop_kind));
        }
    }
    if let Some(values) = get(entries, "additionalProperties") {
        let at = child(path, "additionalProperties");
        if kind != "object" {
            return Err(error(
                at,
                values.offset,
                CrdErrorKind::Invalid("additionalProperties needs type object"),
            ));
        }
        if get(entries, "properties").is_some() {
            return Err(error(
                at,
                values.offset,
                CrdErrorKind::Invalid("properties and additionalProperties exclude each other"),
            ));
        }
        if values.boolean().is_some() {
            return Err(error(
                at,
                values.offset,
                CrdErrorKind::Unsupported("a boolean additionalProperties"),
            ));
        }
        read_schema(
            reader,
            values,
            &at,
            &format!("{id}{{}}"),
            Some((id, "values", None)),
        )?;
    }
    let mut item_properties = None;
    match get(entries, "items") {
        Some(items) => {
            let at = child(path, "items");
            if kind != "array" {
                return Err(error(
                    at,
                    items.offset,
                    CrdErrorKind::Invalid("items need type array"),
                ));
            }
            if items.sequence().is_some() {
                return Err(error(
                    at,
                    items.offset,
                    CrdErrorKind::Invalid("items must be one schema"),
                ));
            }
            let items_id = format!("{id}[]");
            read_schema(reader, items, &at, &items_id, Some((id, "items", None)))?;
            item_properties = Some((items_id, items));
        }
        None if kind == "array" => {
            return Err(error(
                path.to_owned(),
                node.offset,
                CrdErrorKind::Missing("items"),
            ));
        }
        None => {}
    }
    Ok((properties, item_properties))
}

/// Resolve the `required` entries of schema node `id` against its
/// properties.
fn read_required(
    reader: &mut Reader,
    entries: &[(String, Node)],
    path: &str,
    id: &str,
    properties: &[(String, &'static str)],
) {
    if let Some(list) = get(entries, "required") {
        let at = child(path, "required");
        let mut edges = Vec::new();
        for (i, item) in list.sequence().unwrap_or_default().iter().enumerate() {
            let name = item.string().unwrap_or_default();
            if properties.iter().any(|(p, _)| p == name) {
                edges.push(Edge {
                    src: Name::from(id),
                    tgt: Name::from(format!("{id}.{}", escape_segment(name))),
                    kind: Name::from("prop"),
                    name: Some(Name::from(name)),
                });
            } else {
                reader.unresolved.push(UnresolvedRequired {
                    path: format!("{at}[{i}]"),
                    offset: item.offset,
                    name: name.to_owned(),
                });
            }
        }
        if !edges.is_empty() {
            reader.with(|b| b.required(id, edges));
        }
    }
}

/// Read version `i` of `spec.versions`, returning whether it is the storage
/// version.
fn read_version(
    reader: &mut Reader,
    crd: &str,
    version: &Node,
    i: usize,
    seen: &mut BTreeSet<String>,
) -> R<bool> {
    let path = "spec.versions";
    let mut storage = false;
    let at = format!("{path}[{i}]");
    let entries = mapping(version, &at)?;
    known_keys(
        entries,
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
        &at,
    )?;
    let name_node = required(entries, "name", &at, version.offset)?;
    let name_at = child(&at, "name");
    let name = string(name_node, &name_at)?;
    if !dns_label(name) {
        return Err(error(
            name_at,
            name_node.offset,
            CrdErrorKind::Invalid("the name must be a lowercase RFC 1035 label"),
        ));
    }
    if !seen.insert(name.to_owned()) {
        return Err(error(
            name_at,
            name_node.offset,
            CrdErrorKind::DuplicateVersion(name.to_owned()),
        ));
    }
    let id = format!("{crd}/{name}");
    reader.try_with(&name_at, name_node.offset, |b| {
        b.vertex(&id, "version", None)?
            .edge(crd, &id, "version", Some(name))
    })?;
    for key in ["served", "storage"] {
        let flag = required(entries, key, &at, version.offset)?;
        let flag_at = child(&at, key);
        if boolean(flag, &flag_at)? && key == "storage" {
            storage = true;
        }
        reader.constraint(&id, key, flag, &flag_at)?;
    }
    if let Some(deprecated) = get(entries, "deprecated") {
        let deprecated_at = child(&at, "deprecated");
        boolean(deprecated, &deprecated_at)?;
        reader.constraint(&id, "deprecated", deprecated, &deprecated_at)?;
    }
    if let Some(warning) = get(entries, "deprecationWarning") {
        let warning_at = child(&at, "deprecationWarning");
        string(warning, &warning_at)?;
        reader.constraint(&id, "deprecationWarning", warning, &warning_at)?;
    }
    let schema = required(entries, "schema", &at, version.offset)?;
    let schema_at = child(&at, "schema");
    let schema_entries = mapping(schema, &schema_at)?;
    known_keys(schema_entries, &["openAPIV3Schema"], &schema_at)?;
    let root = required(schema_entries, "openAPIV3Schema", &schema_at, schema.offset)?;
    let root_at = child(&schema_at, "openAPIV3Schema");
    let root_id = format!("{id}#");
    let root_kind = read_schema(reader, root, &root_at, &root_id, None)?;
    if root_kind != "object" {
        return Err(error(
            root_at,
            root.offset,
            CrdErrorKind::Invalid("the root schema must have type object"),
        ));
    }
    reader.try_with(&schema_at, schema.offset, |b| {
        b.edge(&id, &root_id, "schema", None)
    })?;
    if let Some(subresources) = get(entries, "subresources") {
        let sub_at = child(&at, "subresources");
        mapping(subresources, &sub_at)?;
        reader.constraint(&id, "subresources", subresources, &sub_at)?;
    }
    for key in ["additionalPrinterColumns", "selectableFields"] {
        if let Some(list) = get(entries, key) {
            let list_at = child(&at, key);
            for (j, item) in sequence(list, &list_at)?.iter().enumerate() {
                mapping(item, &format!("{list_at}[{j}]"))?;
            }
            reader.constraint(&id, key, list, &list_at)?;
        }
    }
    Ok(storage)
}

/// Check the value of a kept keyword: its JSON kind, and the rules that tie
/// it to the node's kind.
fn check_keyword(key: &str, value: &Node, at: &str, kind: &str) -> R<()> {
    let fail = |k: CrdErrorKind| Err(error(at.to_owned(), value.offset, k));
    let number = matches!(
        &value.content,
        Content::Scalar(
            panproto_inst::Value::Int(_)
                | panproto_inst::Value::BigInt(_)
                | panproto_inst::Value::Float(_)
                | panproto_inst::Value::Decimal(_)
        )
    );
    let count = matches!(&value.content, Content::Scalar(panproto_inst::Value::Int(n)) if *n >= 0);
    match key {
        "description" | "title" | "format" | "pattern" => {
            string(value, at)?;
        }
        "nullable"
        | "exclusiveMaximum"
        | "exclusiveMinimum"
        | "x-kubernetes-preserve-unknown-fields"
        | "x-kubernetes-embedded-resource"
        | "x-kubernetes-int-or-string" => {
            boolean(value, at)?;
        }
        "uniqueItems" => {
            if boolean(value, at)? {
                return fail(CrdErrorKind::Forbidden("uniqueItems: true"));
            }
        }
        "maximum" | "minimum" | "multipleOf" if !number => {
            return fail(CrdErrorKind::WrongType("a number"));
        }
        "maxLength" | "minLength" | "maxItems" | "minItems" | "maxProperties" | "minProperties"
            if !count =>
        {
            return fail(CrdErrorKind::WrongType("a non-negative integer"));
        }
        "enum" | "allOf" | "anyOf" | "oneOf" | "x-kubernetes-validations" => {
            sequence(value, at)?;
        }
        "required" | "x-kubernetes-list-map-keys" => {
            for (i, item) in sequence(value, at)?.iter().enumerate() {
                string(item, &format!("{at}[{i}]"))?;
            }
        }
        "not" | "externalDocs" => {
            mapping(value, at)?;
        }
        "x-kubernetes-list-type" => {
            if !matches!(string(value, at)?, "atomic" | "set" | "map") {
                return fail(CrdErrorKind::Invalid(
                    "the list type must be atomic, set or map",
                ));
            }
            if kind != "array" {
                return fail(CrdErrorKind::Invalid(
                    "x-kubernetes-list-type needs type array",
                ));
            }
        }
        "x-kubernetes-map-type" => {
            if !matches!(string(value, at)?, "atomic" | "granular") {
                return fail(CrdErrorKind::Invalid(
                    "the map type must be atomic or granular",
                ));
            }
            if kind != "object" {
                return fail(CrdErrorKind::Invalid(
                    "x-kubernetes-map-type needs type object",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Check `x-kubernetes-list-map-keys` against the list type and the items'
/// properties, and declare the list map's key on the items vertex.
fn read_list_keys(
    reader: &mut Reader,
    entries: &[(String, Node)],
    path: &str,
    id: &str,
    items: Option<(String, &Node)>,
) -> R<()> {
    let list_type = get(entries, "x-kubernetes-list-type").and_then(Node::string);
    let keys = get(entries, "x-kubernetes-list-map-keys");
    match (list_type, keys) {
        (Some("map"), None) => Err(error(
            child(path, "x-kubernetes-list-type"),
            get(entries, "x-kubernetes-list-type").map_or(0, |n| n.offset),
            CrdErrorKind::Missing("x-kubernetes-list-map-keys"),
        )),
        (Some("map"), Some(keys)) => {
            let at = child(path, "x-kubernetes-list-map-keys");
            let names = keys.sequence().unwrap_or_default();
            if names.is_empty() {
                return Err(error(
                    at,
                    keys.offset,
                    CrdErrorKind::Invalid("a list map needs at least one key"),
                ));
            }
            let Some((items_id, items_node)) = items else {
                return Ok(());
            };
            let item_props = items_node
                .mapping()
                .and_then(|e| get(e, "properties"))
                .and_then(Node::mapping)
                .unwrap_or_default();
            let mut parts = Vec::new();
            for (i, key) in names.iter().enumerate() {
                let name = key.string().unwrap_or_default();
                let scalar = get(item_props, name)
                    .and_then(Node::mapping)
                    .and_then(|e| get(e, "type"))
                    .and_then(Node::string)
                    .is_some_and(|t| matches!(t, "string" | "integer" | "number" | "boolean"));
                if !scalar {
                    return Err(error(
                        format!("{at}[{i}]"),
                        key.offset,
                        CrdErrorKind::ListMapKey(name.to_owned()),
                    ));
                }
                parts.push(KeyPart::Path(vec![Edge {
                    src: Name::from(items_id.as_str()),
                    tgt: Name::from(format!("{items_id}.{}", escape_segment(name))),
                    kind: Name::from("prop"),
                    name: Some(Name::from(name)),
                }]));
            }
            let key = Key {
                id: Name::from("list-map-keys"),
                parts,
                role: KeyRole::Unique,
                scope: KeyScope::Within(Name::from(id)),
                equality: Vec::new(),
            };
            reader.with(|b| b.key(&items_id, key));
            Ok(())
        }
        (_, Some(keys)) => Err(error(
            child(path, "x-kubernetes-list-map-keys"),
            keys.offset,
            CrdErrorKind::Invalid("x-kubernetes-list-map-keys needs list type map"),
        )),
        _ => Ok(()),
    }
}

/// Why [`write_crd`] cannot write a schema.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CrdWriteError {
    /// The schema has no `custom-resource` vertex, or more than one.
    #[error("the schema must have exactly one custom-resource vertex")]
    Resource,
    /// A vertex the writer reached has a constraint whose value is not
    /// JSON text.
    #[error("constraint {sort} of {vertex} is not JSON text")]
    Constraint {
        /// The vertex.
        vertex: String,
        /// The constraint's sort.
        sort: String,
    },
    /// A vertex or edge has no place in a CRD.
    #[error("{0} has no place in a CustomResourceDefinition")]
    Structure(String),
}

/// Write a `k8s-crd` schema as its canonical CRD.
///
/// The canonical CRD is one line of JSON with no whitespace between
/// tokens, ended by a line feed. The fields come in the
/// order the CRD definition lists them (`apiVersion`, `kind`, `metadata`
/// with `name` first, `spec` with `group`, `names`, `scope`, `versions`,
/// `conversion` and `preserveUnknownFields`, then `status`); a schema node
/// writes `type`, then its kept constraints in the order the schema holds
/// them, then `properties`, `additionalProperties` and `items`; and each
/// kept constraint is written as its JSON text. Reading the text back with
/// [`read_crd`] gives the schema the text was written from.
///
/// # Errors
///
/// Returns a [`CrdWriteError`] for a schema no CRD reads as.
pub fn write_crd(schema: &Schema) -> Result<String, CrdWriteError> {
    let mut resources = schema
        .vertices
        .values()
        .filter(|v| v.kind.as_ref() == "custom-resource");
    let resource = resources.next().ok_or(CrdWriteError::Resource)?;
    if resources.next().is_some() {
        return Err(CrdWriteError::Resource);
    }
    let crd = resource.id.to_string();
    let mut metadata = vec![("name".to_owned(), json_string(&crd))];
    if let Some(rest) = kept(schema, &crd, "metadata")? {
        let inner = rest
            .strip_prefix('{')
            .and_then(|r| r.strip_suffix('}'))
            .filter(|r| !r.is_empty())
            .ok_or_else(|| CrdWriteError::Constraint {
                vertex: crd.clone(),
                sort: "metadata".into(),
            })?;
        metadata.push((String::new(), inner.to_owned()));
    }
    let mut names = Vec::new();
    for key in [
        "plural",
        "singular",
        "kind",
        "listKind",
        "shortNames",
        "categories",
    ] {
        push_kept(&mut names, schema, &crd, key)?;
    }
    let mut spec = Vec::new();
    push_kept(&mut spec, schema, &crd, "group")?;
    spec.push(("names".to_owned(), object(&names)));
    push_kept(&mut spec, schema, &crd, "scope")?;
    let mut versions = Vec::new();
    for (edge, version) in out_edges(schema, &crd, "version") {
        let mut fields = vec![(
            "name".to_owned(),
            json_string(edge.name.as_deref().unwrap_or_default()),
        )];
        for key in ["served", "storage", "deprecated", "deprecationWarning"] {
            push_kept(&mut fields, schema, &version, key)?;
        }
        let (_, root) = out_edges(schema, &version, "schema")
            .into_iter()
            .next()
            .ok_or_else(|| CrdWriteError::Structure(version.clone()))?;
        let root_text = write_schema(schema, &root)?;
        fields.push((
            "schema".to_owned(),
            object(&[("openAPIV3Schema".to_owned(), root_text)]),
        ));
        for key in [
            "subresources",
            "additionalPrinterColumns",
            "selectableFields",
        ] {
            push_kept(&mut fields, schema, &version, key)?;
        }
        versions.push(object(&fields));
    }
    spec.push(("versions".to_owned(), format!("[{}]", versions.join(","))));
    push_kept(&mut spec, schema, &crd, "conversion")?;
    push_kept(&mut spec, schema, &crd, "preserveUnknownFields")?;
    let mut doc = vec![
        ("apiVersion".to_owned(), json_string(API_VERSION)),
        ("kind".to_owned(), json_string("CustomResourceDefinition")),
        ("metadata".to_owned(), object(&metadata)),
        ("spec".to_owned(), object(&spec)),
    ];
    push_kept(&mut doc, schema, &crd, "status")?;
    let mut text = object(&doc);
    text.push('\n');
    Ok(text)
}

/// The JSON text of the object with `fields`, each a key and the JSON text
/// of its value; a field with an empty key is spliced in as written.
fn object(fields: &[(String, String)]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|(key, value)| {
            if key.is_empty() {
                value.clone()
            } else {
                format!("{}:{value}", json_string(key))
            }
        })
        .collect();
    format!("{{{}}}", parts.join(","))
}

/// The JSON text of `vertex`'s constraint `sort`, checked to be JSON.
fn kept(schema: &Schema, vertex: &str, sort: &str) -> Result<Option<String>, CrdWriteError> {
    let Some(constraint) = schema
        .constraints
        .get(vertex)
        .and_then(|cs| cs.iter().find(|c| c.sort.as_ref() == sort))
    else {
        return Ok(None);
    };
    serde_json::from_str::<serde_json::Value>(&constraint.value).map_err(|_| {
        CrdWriteError::Constraint {
            vertex: vertex.to_owned(),
            sort: sort.to_owned(),
        }
    })?;
    Ok(Some(constraint.value.clone()))
}

fn push_kept(
    fields: &mut Vec<(String, String)>,
    schema: &Schema,
    vertex: &str,
    sort: &str,
) -> Result<(), CrdWriteError> {
    if let Some(text) = kept(schema, vertex, sort)? {
        fields.push((sort.to_owned(), text));
    }
    Ok(())
}

/// The out-edges of `vertex` of kind `kind`, with their targets, in the
/// order the schema lists them.
fn out_edges(schema: &Schema, vertex: &str, kind: &str) -> Vec<(Edge, String)> {
    schema
        .outgoing_edges(vertex)
        .iter()
        .filter(|e| e.kind.as_ref() == kind)
        .map(|e| (e.clone(), e.tgt.to_string()))
        .collect()
}

fn write_schema(schema: &Schema, id: &str) -> Result<String, CrdWriteError> {
    let vertex = schema
        .vertices
        .get(id)
        .ok_or_else(|| CrdWriteError::Structure(id.to_owned()))?;
    let mut fields = Vec::new();
    if TYPES.contains(&vertex.kind.as_ref()) {
        fields.push(("type".to_owned(), json_string(&vertex.kind)));
    }
    for constraint in schema.constraints.get(id).into_iter().flatten() {
        let sort = constraint.sort.to_string();
        if let Some(text) = kept(schema, id, &sort)? {
            fields.push((sort, text));
        }
    }
    let props = out_edges(schema, id, "prop");
    if !props.is_empty() {
        let mut properties = Vec::new();
        for (edge, target) in props {
            properties.push((
                edge.name.as_deref().unwrap_or_default().to_owned(),
                write_schema(schema, &target)?,
            ));
        }
        fields.push(("properties".to_owned(), object(&properties)));
    }
    for (kind, key) in [("values", "additionalProperties"), ("items", "items")] {
        if let Some((_, target)) = out_edges(schema, id, kind).into_iter().next() {
            fields.push((key.to_owned(), write_schema(schema, &target)?));
        }
    }
    Ok(object(&fields))
}
