//! The AWS `CloudFormation` template reader and its canonical writer.
//!
//! [`read_template`] reads one template, as the `2010-09-09` template format
//! defines it, through a shared route (a template whose first byte other
//! than whitespace is `{` through the JSON route [`super::tree::read_json`],
//! any other through the YAML route [`super::tree::read_yaml`]) into a schema
//! of the `cloudformation` protocol, and refuses with a [`CfnError`] naming
//! the field at fault (its path and byte offset) every template the reader
//! does not carry exactly. [`write_template`] writes such a schema back as
//! the canonical template: one line of JSON.
//!
//! The vertex ids are `Template` for the template, `<Section>.<name>` for a
//! parameter, mapping, condition, resource or output (`Parameters.Bucket`,
//! `Resources.Bucket`), and `Pseudo.<name>` for a pseudo parameter a
//! reference names (`Pseudo.AWS::Region`).

use std::collections::BTreeSet;
use std::fmt;

use panproto_inst::Value;
use panproto_schema::{Schema, SchemaBuilder};

use crate::error::ParseInstanceError;

use super::tree::{Content, JsonTextError, Node, json_string, read_json, read_yaml};

/// The template format version the reader reads.
pub const FORMAT_VERSION: &str = "2010-09-09";

/// The pseudo parameters a `Ref` or an `Fn::Sub` variable may name.
pub const PSEUDO_PARAMETERS: &[&str] = &[
    "AWS::AccountId",
    "AWS::NotificationARNs",
    "AWS::NoValue",
    "AWS::Partition",
    "AWS::Region",
    "AWS::StackId",
    "AWS::StackName",
    "AWS::URLSuffix",
];

/// The top-level sections of a template.
const SECTIONS: &[&str] = &[
    "AWSTemplateFormatVersion",
    "Description",
    "Metadata",
    "Parameters",
    "Rules",
    "Mappings",
    "Conditions",
    "Transform",
    "Resources",
    "Outputs",
];

/// The fields of a parameter declaration.
const PARAMETER_FIELDS: &[&str] = &[
    "AllowedPattern",
    "AllowedValues",
    "ConstraintDescription",
    "Default",
    "Description",
    "MaxLength",
    "MaxValue",
    "MinLength",
    "MinValue",
    "NoEcho",
    "Type",
];

/// The fields of a resource declaration.
const RESOURCE_FIELDS: &[&str] = &[
    "Type",
    "Properties",
    "DependsOn",
    "Condition",
    "Metadata",
    "CreationPolicy",
    "DeletionPolicy",
    "UpdatePolicy",
    "UpdateReplacePolicy",
];

/// The fields of an output declaration.
const OUTPUT_FIELDS: &[&str] = &["Description", "Value", "Export", "Condition"];

/// The intrinsic functions the reader resolves or walks, with the number of
/// items their sequence argument holds (`None` for any argument).
const FUNCTIONS: &[(&str, Option<(usize, usize)>)] = &[
    ("Fn::And", Some((2, 10))),
    ("Fn::Base64", None),
    ("Fn::Cidr", Some((3, 3))),
    ("Fn::Equals", Some((2, 2))),
    ("Fn::FindInMap", Some((3, 3))),
    ("Fn::GetAtt", None),
    ("Fn::GetAZs", None),
    ("Fn::If", Some((3, 3))),
    ("Fn::ImportValue", None),
    ("Fn::Join", Some((2, 2))),
    ("Fn::Not", Some((1, 1))),
    ("Fn::Or", Some((2, 10))),
    ("Fn::Select", Some((2, 2))),
    ("Fn::Split", Some((2, 2))),
    ("Fn::Sub", None),
];

/// The functions of the `AWS::LanguageExtensions` transform.
const LANGUAGE_EXTENSIONS: &[&str] = &["Fn::Length", "Fn::ToJsonString", "Fn::ForEach"];

/// The functions a condition declaration may be.
const CONDITION_FUNCTIONS: &[&str] = &[
    "Fn::And",
    "Fn::Equals",
    "Fn::If",
    "Fn::Not",
    "Fn::Or",
    "Condition",
];

/// The short-form tags the reader reads as their full form.
const SHORT_FORMS: &[&str] = &[
    "Ref",
    "Condition",
    "GetAtt",
    "Sub",
    "Base64",
    "GetAZs",
    "ImportValue",
    "And",
    "Cidr",
    "Equals",
    "FindInMap",
    "If",
    "Join",
    "Not",
    "Or",
    "Select",
    "Split",
];

/// A refusal of the template reader: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CfnError {
    /// The path of the field at fault, keys joined by `.` and sequence
    /// indices as `[i]`; empty for the document itself.
    pub path: String,
    /// The byte offset at which the field's value starts.
    pub offset: usize,
    /// What is wrong.
    pub kind: CfnErrorKind,
}

impl fmt::Display for CfnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if self.path.is_empty() {
            "the document"
        } else {
            &self.path
        };
        write!(f, "{path} (byte {}): {}", self.offset, self.kind)
    }
}

impl std::error::Error for CfnError {}

/// What a [`CfnError`] refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CfnErrorKind {
    /// The shared JSON or YAML route refuses the document.
    #[error("the route refuses the document: {0}")]
    Syntax(String),
    /// `AWSTemplateFormatVersion` names another version.
    #[error("AWSTemplateFormatVersion is {0:?}, not 2010-09-09")]
    FormatVersion(String),
    /// A required field is absent.
    #[error("the required field {0} is missing")]
    Missing(&'static str),
    /// A field holds a value of another kind.
    #[error("expected {0}")]
    WrongType(&'static str),
    /// A field breaks a rule of the template format, which the message
    /// states.
    #[error("{0}")]
    Invalid(&'static str),
    /// A field the template format does not declare.
    #[error("unknown field {0:?}")]
    UnknownField(String),
    /// A feature the reader does not carry.
    #[error("{0} is not supported")]
    Unsupported(&'static str),
    /// A one-key mapping whose key names no intrinsic function.
    #[error("{0} is not an intrinsic function")]
    UnknownFunction(String),
    /// The arguments of an intrinsic function do not have its form.
    #[error("the arguments of {0} do not have its form")]
    Arguments(String),
    /// A short-form tag on a sequence or mapping (`!Join [..]`), which the
    /// shared YAML route carries on scalars only.
    #[error("the short-form tag {0} on a sequence or mapping is not supported")]
    TaggedCollection(String),
    /// A local tag that names no short form.
    #[error("the tag !{0} names no short form")]
    UnknownTag(String),
    /// A logical ID declared both as a parameter and as a resource.
    #[error("{0:?} is declared both as a parameter and as a resource")]
    DuplicateName(String),
    /// A reference whose name the template does not declare.
    #[error("{0:?} names nothing the reference can reach")]
    Unresolved(String),
    /// The schema the template describes cannot be built.
    #[error("the schema cannot be built: {0}")]
    Schema(String),
}

/// An `Fn::ImportValue`: a reference to an export of another stack, which
/// no template resolves alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// The path of the `Fn::ImportValue` function.
    pub path: String,
    /// The byte offset of its argument.
    pub offset: usize,
    /// The export name when it is written as a string, `None` when it is
    /// computed by another function.
    pub name: Option<String>,
}

/// What [`read_template`] reads: the schema, and the imports, which name
/// exports outside the template, in document order.
#[derive(Clone, Debug)]
pub struct CfnReading {
    /// The schema of the `cloudformation` protocol.
    pub schema: Schema,
    /// The `Fn::ImportValue` references, which stay unresolved.
    pub imports: Vec<Import>,
}

type R<T> = Result<T, CfnError>;

const fn error(path: String, offset: usize, kind: CfnErrorKind) -> CfnError {
    CfnError { path, offset, kind }
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

fn mapping<'n>(node: &'n Node, path: &str) -> R<&'n [(String, Node)]> {
    node.mapping().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            CfnErrorKind::WrongType("a mapping"),
        )
    })
}

fn sequence<'n>(node: &'n Node, path: &str) -> R<&'n [Node]> {
    node.sequence().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            CfnErrorKind::WrongType("a sequence"),
        )
    })
}

fn string<'n>(node: &'n Node, path: &str) -> R<&'n str> {
    node.string().ok_or_else(|| {
        error(
            path.to_owned(),
            node.offset,
            CfnErrorKind::WrongType("a string"),
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
                CfnErrorKind::UnknownField(key.clone()),
            ));
        }
    }
    Ok(())
}

/// Whether `text` is a logical ID: one or more ASCII letters and digits.
fn alphanumeric(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Whether `text` is a parameter type the template format defines.
fn parameter_type(text: &str) -> bool {
    matches!(
        text,
        "String" | "Number" | "List<Number>" | "CommaDelimitedList"
    ) || text.starts_with("AWS::")
        || (text.starts_with("List<AWS::") && text.ends_with('>'))
}

/// Read `source`, one `CloudFormation` template in JSON or YAML, into a
/// `cloudformation` schema.
///
/// The order of the checks, and so which refusal comes first, is the one
/// the `CloudFormation` page of the book states: the route, the short-form
/// tags, the top-level fields, the sections' names, then each section's
/// declarations in the order Parameters, Mappings, Conditions, Resources,
/// Outputs.
///
/// # Errors
///
/// Returns a [`CfnError`] naming the field at fault.
pub fn read_template(source: &[u8]) -> Result<CfnReading, CfnError> {
    let json = source
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{');
    let document = if json {
        read_json(source)
    } else {
        read_yaml(source)
    }
    .map_err(|e| match e {
        ParseInstanceError::UnsupportedTaggedContainer {
            tag, byte_offset, ..
        } => error(
            String::new(),
            byte_offset
                .and_then(|o| usize::try_from(o).ok())
                .unwrap_or_default(),
            CfnErrorKind::TaggedCollection(tag),
        ),
        e => error(String::new(), 0, CfnErrorKind::Syntax(e.to_string())),
    })?;
    let document = full_form(&document, "")?;
    read_document(&document)
}

/// The document with every short-form tag replaced by its full form: a
/// scalar `!Ref x` by the mapping `{Ref: x}`, `!Condition x` by
/// `{Condition: x}`, and `!F x` by `{Fn::F: x}` for the other short forms.
fn full_form(node: &Node, path: &str) -> R<Node> {
    let content = match &node.content {
        Content::Scalar(Value::Opaque { type_, fields }) => {
            let tag = type_.trim_start_matches('!');
            if !SHORT_FORMS.contains(&tag) {
                return Err(error(
                    path.to_owned(),
                    node.offset,
                    CfnErrorKind::UnknownTag(tag.to_owned()),
                ));
            }
            let key = if matches!(tag, "Ref" | "Condition") {
                tag.to_owned()
            } else {
                format!("Fn::{tag}")
            };
            let payload = match fields.get("value") {
                Some(Value::Str(text)) => text.clone(),
                _ => String::new(),
            };
            let argument = Node {
                content: Content::Scalar(Value::Str(payload)),
                offset: node.offset,
            };
            Content::Mapping(vec![(key, argument)])
        }
        Content::Scalar(value) => Content::Scalar(value.clone()),
        Content::Sequence(items) => Content::Sequence(
            items
                .iter()
                .enumerate()
                .map(|(i, item)| full_form(item, &format!("{path}[{i}]")))
                .collect::<R<_>>()?,
        ),
        Content::Mapping(entries) => Content::Mapping(
            entries
                .iter()
                .map(|(key, value)| Ok((key.clone(), full_form(value, &child(path, key))?)))
                .collect::<R<_>>()?,
        ),
    };
    Ok(Node {
        content,
        offset: node.offset,
    })
}

/// The sections that declare logical IDs, with the vertex kind each
/// declaration reads as, in the order the reader reads them.
const DECLARING: &[(&str, &str)] = &[
    ("Parameters", "parameter"),
    ("Mappings", "mapping"),
    ("Conditions", "condition"),
    ("Resources", "resource"),
    ("Outputs", "output"),
];

/// The logical IDs each section declares.
#[derive(Default)]
struct Names {
    parameters: BTreeSet<String>,
    mappings: BTreeSet<String>,
    conditions: BTreeSet<String>,
    resources: BTreeSet<String>,
}

/// Where a reference stands: in a resource or output, or in a condition,
/// which may reference only parameters and other conditions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Resource,
    Condition,
}

/// The reader's state.
struct Reader {
    builder: Option<SchemaBuilder>,
    names: Names,
    imports: Vec<Import>,
    /// The edges added so far, to add each one once.
    edges: BTreeSet<(String, String, String, Option<String>)>,
    /// The pseudo parameters given a vertex so far.
    pseudo: BTreeSet<String>,
}

impl Reader {
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
            .map_err(|e| error(path.to_owned(), offset, CfnErrorKind::Schema(e.to_string())))?;
        self.builder = Some(built);
        Ok(())
    }

    fn constraint(&mut self, vertex: &str, sort: &str, node: &Node, path: &str) -> R<()> {
        let text = json(node, path)?;
        self.builder = self
            .builder
            .take()
            .map(|b| b.constraint(vertex, sort, &text));
        Ok(())
    }

    /// Add the edge once; a repeated reference adds nothing.
    fn edge(
        &mut self,
        src: &str,
        tgt: &str,
        kind: &str,
        name: Option<&str>,
        at: (&str, usize),
    ) -> R<()> {
        let key = (
            src.to_owned(),
            tgt.to_owned(),
            kind.to_owned(),
            name.map(str::to_owned),
        );
        if self.edges.insert(key) {
            self.try_with(at.0, at.1, |b| b.edge(src, tgt, kind, name))?;
        }
        Ok(())
    }

    /// The vertex of pseudo parameter `name`, added on its first use.
    fn pseudo(&mut self, name: &str, at: (&str, usize)) -> R<String> {
        let id = format!("Pseudo.{name}");
        if self.pseudo.insert(name.to_owned()) {
            self.try_with(at.0, at.1, |b| b.vertex(&id, "pseudo-parameter", None))?;
        }
        Ok(id)
    }
}

/// `node`'s canonical JSON text, or the refusal of the scalar JSON cannot
/// write.
fn json(node: &Node, path: &str) -> R<String> {
    node.json_text().map_err(|e: JsonTextError| {
        error(
            path.to_owned(),
            node.offset,
            CfnErrorKind::Unsupported(e.feature()),
        )
    })
}

/// Read a document tree, with its short forms in full form, into a
/// `cloudformation` schema, as [`read_template`] does.
///
/// # Errors
///
/// Returns a [`CfnError`] naming the field at fault.
pub fn read_document(document: &Node) -> Result<CfnReading, CfnError> {
    let top = mapping(document, "")?;
    known_keys(top, SECTIONS, "")?;
    let proto = panproto_protocols::config::cloudformation::protocol();
    let mut reader = Reader {
        builder: Some(SchemaBuilder::new(&proto)),
        names: Names::default(),
        imports: Vec::new(),
        edges: BTreeSet::new(),
        pseudo: BTreeSet::new(),
    };
    reader.try_with("", document.offset, |b| {
        b.vertex("Template", "template", None)
    })?;
    read_header(&mut reader, top)?;
    let Some(resources) = get(top, "Resources") else {
        return Err(error(
            String::new(),
            document.offset,
            CfnErrorKind::Missing("Resources"),
        ));
    };
    declare(&mut reader, top)?;
    if reader.names.resources.is_empty() {
        return Err(error(
            "Resources".into(),
            resources.offset,
            CfnErrorKind::Invalid("a template declares at least one resource"),
        ));
    }
    for (section, _) in DECLARING {
        let Some(node) = get(top, section) else {
            continue;
        };
        for (name, body) in node.mapping().unwrap_or_default() {
            let path = format!("{section}.{name}");
            let id = path.clone();
            match *section {
                "Parameters" => read_parameter(&mut reader, &id, body, &path)?,
                "Mappings" => read_mapping(&mut reader, &id, body, &path)?,
                "Conditions" => read_condition(&mut reader, &id, body, &path)?,
                "Resources" => read_resource(&mut reader, &id, body, &path)?,
                _ => read_output(&mut reader, &id, body, &path)?,
            }
        }
    }
    let schema = reader.builder.take().map_or_else(
        || {
            Err(error(
                String::new(),
                0,
                CfnErrorKind::Schema("no builder".into()),
            ))
        },
        |b| {
            b.build()
                .map_err(|e| error(String::new(), 0, CfnErrorKind::Schema(e.to_string())))
        },
    )?;
    Ok(CfnReading {
        schema,
        imports: reader.imports,
    })
}

/// Read the format version, the refused sections, the description and the
/// metadata, keeping the three kept fields on the template vertex in that
/// order.
fn read_header(reader: &mut Reader, top: &[(String, Node)]) -> R<()> {
    if let Some(version) = get(top, "AWSTemplateFormatVersion") {
        let at = "AWSTemplateFormatVersion";
        let text = string(version, at)?;
        if text != FORMAT_VERSION {
            return Err(error(
                at.into(),
                version.offset,
                CfnErrorKind::FormatVersion(text.to_owned()),
            ));
        }
    }
    if let Some(transform) = get(top, "Transform") {
        return Err(error(
            "Transform".into(),
            transform.offset,
            CfnErrorKind::Unsupported("a transform"),
        ));
    }
    if let Some(rules) = get(top, "Rules") {
        return Err(error(
            "Rules".into(),
            rules.offset,
            CfnErrorKind::Unsupported("the Rules section"),
        ));
    }
    if let Some(description) = get(top, "Description") {
        string(description, "Description")?;
    }
    if let Some(metadata) = get(top, "Metadata") {
        mapping(metadata, "Metadata")?;
    }
    for key in ["AWSTemplateFormatVersion", "Description", "Metadata"] {
        if let Some(node) = get(top, key) {
            reader.constraint("Template", key, node, key)?;
        }
    }
    Ok(())
}

/// Check each declaring section's shape and names, then add a vertex and a
/// `declares` edge for every declaration, section by section in source
/// order.
fn declare(reader: &mut Reader, top: &[(String, Node)]) -> R<()> {
    for (section, _) in DECLARING {
        let Some(node) = get(top, section) else {
            continue;
        };
        for (name, body) in mapping(node, section)? {
            if *section != "Conditions" && !alphanumeric(name) {
                return Err(error(
                    child(section, name),
                    body.offset,
                    CfnErrorKind::Invalid("a logical ID must be alphanumeric"),
                ));
            }
            let set = match *section {
                "Parameters" => &mut reader.names.parameters,
                "Mappings" => &mut reader.names.mappings,
                "Conditions" => &mut reader.names.conditions,
                "Resources" => &mut reader.names.resources,
                _ => continue,
            };
            set.insert(name.clone());
        }
    }
    if let Some(resources) = get(top, "Resources") {
        for (name, body) in resources.mapping().unwrap_or_default() {
            if reader.names.parameters.contains(name) {
                return Err(error(
                    child("Resources", name),
                    body.offset,
                    CfnErrorKind::DuplicateName(name.clone()),
                ));
            }
        }
    }
    for (section, kind) in DECLARING {
        let Some(node) = get(top, section) else {
            continue;
        };
        for (name, body) in node.mapping().unwrap_or_default() {
            let id = format!("{section}.{name}");
            reader.try_with(&id, body.offset, |b| {
                b.vertex(&id, kind, None)?
                    .edge("Template", &id, "declares", Some(name))
            })?;
        }
    }
    Ok(())
}

fn read_parameter(reader: &mut Reader, id: &str, body: &Node, path: &str) -> R<()> {
    let entries = mapping(body, path)?;
    known_keys(entries, PARAMETER_FIELDS, path)?;
    let Some(ty) = get(entries, "Type") else {
        return Err(error(
            path.to_owned(),
            body.offset,
            CfnErrorKind::Missing("Type"),
        ));
    };
    let type_at = child(path, "Type");
    if !parameter_type(string(ty, &type_at)?) {
        return Err(error(
            type_at,
            ty.offset,
            CfnErrorKind::Invalid("the type is not a parameter type"),
        ));
    }
    for (key, value) in entries {
        let at = child(path, key);
        match key.as_str() {
            "AllowedValues" => {
                sequence(value, &at)?;
            }
            "Description" | "AllowedPattern" | "ConstraintDescription" => {
                string(value, &at)?;
            }
            _ => {}
        }
        reader.constraint(id, key, value, &at)?;
    }
    Ok(())
}

fn read_mapping(reader: &mut Reader, id: &str, body: &Node, path: &str) -> R<()> {
    for (top_key, second) in mapping(body, path)? {
        let top_at = child(path, top_key);
        for (key, value) in mapping(second, &top_at)? {
            if value.mapping().is_some() {
                return Err(error(
                    child(&top_at, key),
                    value.offset,
                    CfnErrorKind::WrongType("a scalar or a sequence"),
                ));
            }
        }
    }
    reader.constraint(id, "Map", body, path)
}

fn read_condition(reader: &mut Reader, id: &str, body: &Node, path: &str) -> R<()> {
    let entries = mapping(body, path)?;
    let function = matches!(entries, [(key, _)] if CONDITION_FUNCTIONS.contains(&key.as_str()));
    if !function {
        return Err(error(
            path.to_owned(),
            body.offset,
            CfnErrorKind::Invalid("a condition is a condition function"),
        ));
    }
    walk(reader, id, body, path, Context::Condition)?;
    reader.constraint(id, "Definition", body, path)
}

/// Resolve a `Condition` field naming a condition, adding a `condition`
/// edge from `id`.
fn condition_field(reader: &mut Reader, id: &str, node: &Node, path: &str) -> R<()> {
    let name = string(node, path)?;
    if !reader.names.conditions.contains(name) {
        return Err(error(
            path.to_owned(),
            node.offset,
            CfnErrorKind::Unresolved(name.to_owned()),
        ));
    }
    let target = format!("Conditions.{name}");
    reader.edge(id, &target, "condition", None, (path, node.offset))
}

fn read_resource(reader: &mut Reader, id: &str, body: &Node, path: &str) -> R<()> {
    let entries = mapping(body, path)?;
    known_keys(entries, RESOURCE_FIELDS, path)?;
    let Some(ty) = get(entries, "Type") else {
        return Err(error(
            path.to_owned(),
            body.offset,
            CfnErrorKind::Missing("Type"),
        ));
    };
    resource_type(ty, &child(path, "Type"))?;
    if let Some(condition) = get(entries, "Condition") {
        condition_field(reader, id, condition, &child(path, "Condition"))?;
    }
    if let Some(depends) = get(entries, "DependsOn") {
        depends_on(reader, id, depends, &child(path, "DependsOn"))?;
    }
    for (key, value) in entries {
        let at = child(path, key);
        match key.as_str() {
            "Condition" | "DependsOn" => continue,
            "Type" => {}
            "DeletionPolicy" => {
                let policy = string(value, &at)?;
                if !matches!(
                    policy,
                    "Delete" | "Retain" | "RetainExceptOnCreate" | "Snapshot"
                ) {
                    return Err(error(
                        at,
                        value.offset,
                        CfnErrorKind::Invalid("the deletion policy is not a policy"),
                    ));
                }
            }
            "UpdateReplacePolicy" => {
                let policy = string(value, &at)?;
                if !matches!(policy, "Delete" | "Retain" | "Snapshot") {
                    return Err(error(
                        at,
                        value.offset,
                        CfnErrorKind::Invalid("the update replace policy is not a policy"),
                    ));
                }
            }
            _ => {
                mapping(value, &at)?;
                walk(reader, id, value, &at, Context::Resource)?;
            }
        }
        reader.constraint(id, key, value, &at)?;
    }
    Ok(())
}

/// Check a resource type: `Custom::<name>`, or three non-empty segments
/// joined by `::`; a fourth segment `MODULE` names a module, which is
/// refused.
fn resource_type(ty: &Node, at: &str) -> R<()> {
    let text = string(ty, at)?;
    let segments: Vec<&str> = text.split("::").collect();
    let nonempty = segments.iter().all(|s| !s.is_empty());
    if segments.len() == 4 && nonempty && segments[3] == "MODULE" {
        return Err(error(
            at.to_owned(),
            ty.offset,
            CfnErrorKind::Unsupported("a module"),
        ));
    }
    let custom = segments.len() == 2 && segments[0] == "Custom" && nonempty;
    if !(custom || (segments.len() == 3 && nonempty)) {
        return Err(error(
            at.to_owned(),
            ty.offset,
            CfnErrorKind::Invalid("the type is not a resource type"),
        ));
    }
    Ok(())
}

/// Resolve `DependsOn`, a resource name or a sequence of them, adding a
/// `depends-on` edge from `id` to each.
fn depends_on(reader: &mut Reader, id: &str, node: &Node, path: &str) -> R<()> {
    let items: Vec<(String, &Node)> = match &node.content {
        Content::Sequence(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| (format!("{path}[{i}]"), item))
            .collect(),
        _ => vec![(path.to_owned(), node)],
    };
    for (at, item) in items {
        let name = string(item, &at)?;
        if !reader.names.resources.contains(name) {
            return Err(error(
                at,
                item.offset,
                CfnErrorKind::Unresolved(name.to_owned()),
            ));
        }
        let target = format!("Resources.{name}");
        reader.edge(id, &target, "depends-on", None, (&at, item.offset))?;
    }
    Ok(())
}

fn read_output(reader: &mut Reader, id: &str, body: &Node, path: &str) -> R<()> {
    let entries = mapping(body, path)?;
    known_keys(entries, OUTPUT_FIELDS, path)?;
    if get(entries, "Value").is_none() {
        return Err(error(
            path.to_owned(),
            body.offset,
            CfnErrorKind::Missing("Value"),
        ));
    }
    if let Some(condition) = get(entries, "Condition") {
        condition_field(reader, id, condition, &child(path, "Condition"))?;
    }
    for (key, value) in entries {
        let at = child(path, key);
        match key.as_str() {
            "Condition" => continue,
            "Description" => {
                string(value, &at)?;
            }
            "Export" => {
                let fields = mapping(value, &at)?;
                known_keys(fields, &["Name"], &at)?;
                if get(fields, "Name").is_none() {
                    return Err(error(at, value.offset, CfnErrorKind::Missing("Name")));
                }
                walk(reader, id, value, &at, Context::Resource)?;
            }
            _ => walk(reader, id, value, &at, Context::Resource)?,
        }
        reader.constraint(id, key, value, &at)?;
    }
    Ok(())
}

/// Resolve every reference in `node`, depth first in source order, adding
/// an edge from `holder` for each.
fn walk(reader: &mut Reader, holder: &str, node: &Node, path: &str, ctx: Context) -> R<()> {
    match &node.content {
        Content::Scalar(_) => Ok(()),
        Content::Sequence(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(reader, holder, item, &format!("{path}[{i}]"), ctx)?;
            }
            Ok(())
        }
        Content::Mapping(entries) => {
            if let [(key, argument)] = entries.as_slice() {
                let at = child(path, key);
                if key == "Ref" {
                    return reference(reader, holder, argument, &at, ctx);
                }
                if key == "Condition" && ctx == Context::Condition {
                    return uses_condition(reader, holder, argument, &at);
                }
                if key.starts_with("Fn::") {
                    return function(reader, holder, key, argument, &at, ctx);
                }
            }
            for (key, value) in entries {
                walk(reader, holder, value, &child(path, key), ctx)?;
            }
            Ok(())
        }
    }
}

/// Resolve the name of a `Ref` or an `Fn::Sub` variable without a dot to
/// its vertex: a pseudo parameter, a parameter, or (outside a condition) a
/// resource.
fn ref_target(reader: &mut Reader, name: &str, at: (&str, usize), ctx: Context) -> R<String> {
    if PSEUDO_PARAMETERS.contains(&name) {
        return reader.pseudo(name, at);
    }
    if reader.names.parameters.contains(name) {
        return Ok(format!("Parameters.{name}"));
    }
    if reader.names.resources.contains(name) {
        if ctx == Context::Condition {
            return Err(error(
                at.0.to_owned(),
                at.1,
                CfnErrorKind::Invalid("a condition cannot reference a resource"),
            ));
        }
        return Ok(format!("Resources.{name}"));
    }
    Err(error(
        at.0.to_owned(),
        at.1,
        CfnErrorKind::Unresolved(name.to_owned()),
    ))
}

fn reference(reader: &mut Reader, holder: &str, argument: &Node, at: &str, ctx: Context) -> R<()> {
    let name = string(argument, at)?;
    let target = ref_target(reader, name, (at, argument.offset), ctx)?;
    reader.edge(holder, &target, "ref", None, (at, argument.offset))
}

fn uses_condition(reader: &mut Reader, holder: &str, argument: &Node, at: &str) -> R<()> {
    let name = string(argument, at)?;
    if !reader.names.conditions.contains(name) {
        return Err(error(
            at.to_owned(),
            argument.offset,
            CfnErrorKind::Unresolved(name.to_owned()),
        ));
    }
    let target = format!("Conditions.{name}");
    reader.edge(
        holder,
        &target,
        "uses-condition",
        None,
        (at, argument.offset),
    )
}

/// Resolve resource `name`'s attribute `attribute`, adding a `get-att` edge
/// named by the attribute.
fn get_att(
    reader: &mut Reader,
    holder: &str,
    name: &str,
    attribute: &str,
    at: (&str, usize),
    ctx: Context,
) -> R<()> {
    if ctx == Context::Condition {
        return Err(error(
            at.0.to_owned(),
            at.1,
            CfnErrorKind::Invalid("a condition cannot read a resource attribute"),
        ));
    }
    if !reader.names.resources.contains(name) {
        return Err(error(
            at.0.to_owned(),
            at.1,
            CfnErrorKind::Unresolved(name.to_owned()),
        ));
    }
    let target = format!("Resources.{name}");
    reader.edge(holder, &target, "get-att", Some(attribute), at)
}

fn function(
    reader: &mut Reader,
    holder: &str,
    key: &str,
    argument: &Node,
    at: &str,
    ctx: Context,
) -> R<()> {
    if key == "Fn::Transform" {
        return Err(error(
            at.to_owned(),
            argument.offset,
            CfnErrorKind::Unsupported("a macro"),
        ));
    }
    if LANGUAGE_EXTENSIONS
        .iter()
        .any(|f| key == *f || key.starts_with("Fn::ForEach::"))
    {
        return Err(error(
            at.to_owned(),
            argument.offset,
            CfnErrorKind::Unsupported("a language extension function"),
        ));
    }
    let Some((_, arity)) = FUNCTIONS.iter().find(|(f, _)| *f == key) else {
        return Err(error(
            at.to_owned(),
            argument.offset,
            CfnErrorKind::UnknownFunction(key.to_owned()),
        ));
    };
    let bad = || {
        Err(error(
            at.to_owned(),
            argument.offset,
            CfnErrorKind::Arguments(key.to_owned()),
        ))
    };
    if let Some((low, high)) = arity {
        match argument.sequence() {
            Some(items) if (*low..=*high).contains(&items.len()) => {}
            _ => return bad(),
        }
    }
    let items = argument.sequence().unwrap_or_default();
    match key {
        "Fn::GetAtt" => match (&argument.content, items) {
            (Content::Scalar(Value::Str(text)), _) => match text.split_once('.') {
                Some((name, attribute)) => {
                    get_att(reader, holder, name, attribute, (at, argument.offset), ctx)
                }
                None => bad(),
            },
            (_, [name, attribute]) => match (name.string(), attribute.string()) {
                (Some(name), Some(attribute)) => {
                    get_att(reader, holder, name, attribute, (at, argument.offset), ctx)
                }
                _ => bad(),
            },
            _ => bad(),
        },
        "Fn::FindInMap" => find_in_map(reader, holder, items, at, ctx),
        "Fn::If" => {
            if items[0].string().is_none() {
                return bad();
            }
            uses_condition(reader, holder, &items[0], &format!("{at}[0]"))?;
            for (i, item) in items.iter().enumerate().skip(1) {
                walk(reader, holder, item, &format!("{at}[{i}]"), ctx)?;
            }
            Ok(())
        }
        "Fn::Sub" => sub(reader, holder, argument, at, ctx),
        "Fn::ImportValue" => {
            reader.imports.push(Import {
                path: at.to_owned(),
                offset: argument.offset,
                name: argument.string().map(str::to_owned),
            });
            walk(reader, holder, argument, at, ctx)
        }
        _ => walk(reader, holder, argument, at, ctx),
    }
}

fn find_in_map(reader: &mut Reader, holder: &str, items: &[Node], at: &str, ctx: Context) -> R<()> {
    let first = format!("{at}[0]");
    match items[0].string() {
        Some(name) => {
            if !reader.names.mappings.contains(name) {
                return Err(error(
                    first,
                    items[0].offset,
                    CfnErrorKind::Unresolved(name.to_owned()),
                ));
            }
            let target = format!("Mappings.{name}");
            reader.edge(
                holder,
                &target,
                "find-in-map",
                None,
                (&first, items[0].offset),
            )?;
        }
        None => walk(reader, holder, &items[0], &first, ctx)?,
    }
    for (i, item) in items.iter().enumerate().skip(1) {
        walk(reader, holder, item, &format!("{at}[{i}]"), ctx)?;
    }
    Ok(())
}

/// The variables of an `Fn::Sub` string, in order: the text between each
/// `${` and the next `}`, except a `${!` literal.
#[must_use]
pub fn sub_variables(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        if let Some(literal) = after.strip_prefix('!') {
            rest = literal;
            continue;
        }
        let Some(end) = after.find('}') else {
            break;
        };
        out.push(&after[..end]);
        rest = &after[end + 1..];
    }
    out
}

fn sub(reader: &mut Reader, holder: &str, argument: &Node, at: &str, ctx: Context) -> R<()> {
    let bad = || {
        Err(error(
            at.to_owned(),
            argument.offset,
            CfnErrorKind::Arguments("Fn::Sub".to_owned()),
        ))
    };
    let (text, locals) = match &argument.content {
        Content::Scalar(Value::Str(text)) => (text.as_str(), &[][..]),
        Content::Sequence(items) => match items.as_slice() {
            [text, vars] => match (text.string(), vars.mapping()) {
                (Some(text), Some(vars)) => (text, vars),
                _ => return bad(),
            },
            _ => return bad(),
        },
        _ => return bad(),
    };
    let text_at = if locals.is_empty() && argument.string().is_some() {
        at.to_owned()
    } else {
        format!("{at}[0]")
    };
    let offset = argument.offset;
    for variable in sub_variables(text) {
        if get(locals, variable).is_some() {
            continue;
        }
        if !PSEUDO_PARAMETERS.contains(&variable)
            && let Some((name, attribute)) = variable.split_once('.')
        {
            get_att(reader, holder, name, attribute, (&text_at, offset), ctx)?;
            continue;
        }
        let target = ref_target(reader, variable, (&text_at, offset), ctx)?;
        reader.edge(holder, &target, "ref", None, (&text_at, offset))?;
    }
    if let Some(vars) = argument.sequence().and_then(|items| items.get(1)) {
        walk(reader, holder, vars, &format!("{at}[1]"), ctx)?;
    }
    Ok(())
}

/// Why [`write_template`] cannot write a schema.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CfnWriteError {
    /// The schema has no `Template` vertex of kind `template`.
    #[error("the schema has no template vertex")]
    Template,
    /// A vertex the writer reached has a constraint whose value is not
    /// JSON text.
    #[error("constraint {sort} of {vertex} is not JSON text")]
    Constraint {
        /// The vertex.
        vertex: String,
        /// The constraint's sort.
        sort: String,
    },
    /// A vertex or edge has no place in a template.
    #[error("{0} has no place in a template")]
    Structure(String),
}

/// Write a `cloudformation` schema as its canonical template.
///
/// The canonical template is one line of JSON with no whitespace between
/// tokens, ended by a line feed. It holds `AWSTemplateFormatVersion`,
/// `Description` and `Metadata` when the schema keeps them, then the
/// sections `Parameters`, `Mappings`, `Conditions`, `Resources` and
/// `Outputs` that declare anything, each declaration in the order the
/// template vertex's `declares` edges list them. A parameter, resource or
/// output writes its kept fields in the order the schema holds them, then a
/// resource or output its `Condition`, then a resource its `DependsOn` as a
/// sequence; a mapping writes its kept map and a condition its kept
/// definition. Reading the text back with [`read_template`] gives the
/// schema the text was written from.
///
/// # Errors
///
/// Returns a [`CfnWriteError`] for a schema no template reads as.
pub fn write_template(schema: &Schema) -> Result<String, CfnWriteError> {
    if schema
        .vertices
        .get("Template")
        .is_none_or(|v| v.kind.as_ref() != "template")
    {
        return Err(CfnWriteError::Template);
    }
    let mut doc = Vec::new();
    for key in ["AWSTemplateFormatVersion", "Description", "Metadata"] {
        if let Some(text) = kept(schema, "Template", key)? {
            doc.push((key.to_owned(), text));
        }
    }
    let declared: Vec<(String, String, String)> = schema
        .outgoing_edges("Template")
        .iter()
        .filter(|e| e.kind.as_ref() == "declares")
        .map(|e| {
            let kind = schema
                .vertices
                .get(&e.tgt)
                .map(|v| v.kind.to_string())
                .unwrap_or_default();
            (
                kind,
                e.name.as_deref().unwrap_or_default().to_owned(),
                e.tgt.to_string(),
            )
        })
        .collect();
    for (section, kind) in DECLARING {
        let mut entries = Vec::new();
        for (vertex_kind, name, id) in &declared {
            if vertex_kind != kind {
                continue;
            }
            let body = match *kind {
                "mapping" => kept(schema, id, "Map")?,
                "condition" => kept(schema, id, "Definition")?,
                _ => Some(declaration(schema, id)?),
            }
            .ok_or_else(|| CfnWriteError::Structure(id.clone()))?;
            entries.push((name.clone(), body));
        }
        if !entries.is_empty() {
            doc.push(((*section).to_owned(), object(&entries)));
        }
    }
    let mut text = object(&doc);
    text.push('\n');
    Ok(text)
}

/// The body of a parameter, resource or output: its kept fields, then its
/// condition and dependencies.
fn declaration(schema: &Schema, id: &str) -> Result<String, CfnWriteError> {
    let mut fields = Vec::new();
    for constraint in schema.constraints.get(id).into_iter().flatten() {
        let sort = constraint.sort.to_string();
        if let Some(text) = kept(schema, id, &sort)? {
            fields.push((sort, text));
        }
    }
    let targets = |kind: &str| -> Vec<String> {
        schema
            .outgoing_edges(id)
            .iter()
            .filter(|e| e.kind.as_ref() == kind)
            .map(|e| {
                e.tgt
                    .split_once('.')
                    .map_or_else(|| e.tgt.to_string(), |(_, name)| name.to_owned())
            })
            .collect()
    };
    if let Some(condition) = targets("condition").first() {
        fields.push(("Condition".to_owned(), json_string(condition)));
    }
    let depends = targets("depends-on");
    if !depends.is_empty() {
        let items: Vec<String> = depends.iter().map(|d| json_string(d)).collect();
        fields.push(("DependsOn".to_owned(), format!("[{}]", items.join(","))));
    }
    Ok(object(&fields))
}

/// The JSON text of the object with `fields`, each a key and the JSON text
/// of its value.
fn object(fields: &[(String, String)]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{}:{value}", json_string(key)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

/// The JSON text of `vertex`'s constraint `sort`, checked to be JSON.
fn kept(schema: &Schema, vertex: &str, sort: &str) -> Result<Option<String>, CfnWriteError> {
    let Some(constraint) = schema
        .constraints
        .get(vertex)
        .and_then(|cs| cs.iter().find(|c| c.sort.as_ref() == sort))
    else {
        return Ok(None);
    };
    serde_json::from_str::<serde_json::Value>(&constraint.value).map_err(|_| {
        CfnWriteError::Constraint {
            vertex: vertex.to_owned(),
            sort: sort.to_owned(),
        }
    })?;
    Ok(Some(constraint.value.clone()))
}

#[cfg(test)]
mod tests {
    use super::sub_variables;

    #[test]
    fn sub_variables_skip_literals_and_stop_at_an_unclosed_variable() {
        assert_eq!(
            sub_variables("arn:${AWS::Partition}:s3:::${Bucket}/${!Literal}/${Res.Arn}/${open"),
            ["AWS::Partition", "Bucket", "Res.Arn"]
        );
    }
}
