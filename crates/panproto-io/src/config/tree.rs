//! The tree a configuration document reads as through the shared YAML route.
//!
//! A configuration format reads no YAML itself. [`read_yaml`] reads the
//! document with the YAML route of [`UnifiedCodec`] (YAML 1.2.2, one
//! document, the core schema, as the text-formats chapter of the book
//! states), so every refusal of that route (a stream of several documents,
//! an alias, a `%TAG` directive, a document that is not UTF-8) is the
//! configuration reader's refusal too. This module gives the instance tree
//! the route reads the shape a configuration reader walks: each mapping its
//! entries in source order, each sequence its items in order, each scalar
//! its value, and each node the byte offset at which it starts.

use std::collections::HashMap;
use std::fmt::Write as _;

use panproto_inst::{FieldPresence, NodeShape, Value, WInstance};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

use crate::cst_extract::{CstComplement, cst_byte_offset};
use crate::error::ParseInstanceError;
use crate::unified_codec::UnifiedCodec;

/// The protocol name the shared YAML route reads under.
const YAML: &str = "yaml";

/// A node of a configuration document.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// What the node holds.
    pub content: Content,
    /// The byte offset at which the node starts in the source.
    pub offset: usize,
}

/// What a node holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// A scalar, `Value::Null` for a null.
    Scalar(Value),
    /// A sequence's items, in order.
    Sequence(Vec<Node>),
    /// A mapping's entries, in source order.
    Mapping(Vec<(String, Node)>),
}

/// Why a node has no canonical JSON text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonTextError {
    /// A float that is infinite or not a number, which JSON cannot write.
    NonFiniteNumber,
    /// A `!!binary` scalar.
    BinaryScalar,
    /// A scalar with a local tag, or any other value JSON has no form for.
    TaggedScalar,
}

impl JsonTextError {
    /// The feature the error names.
    #[must_use]
    pub const fn feature(self) -> &'static str {
        match self {
            Self::NonFiniteNumber => "a non-finite number",
            Self::BinaryScalar => "a binary scalar",
            Self::TaggedScalar => "a tagged scalar",
        }
    }
}

impl Node {
    /// The entries of a mapping, or `None` for any other node.
    #[must_use]
    pub fn mapping(&self) -> Option<&[(String, Self)]> {
        match &self.content {
            Content::Mapping(entries) => Some(entries),
            _ => None,
        }
    }

    /// The items of a sequence, or `None` for any other node.
    #[must_use]
    pub fn sequence(&self) -> Option<&[Self]> {
        match &self.content {
            Content::Sequence(items) => Some(items),
            _ => None,
        }
    }

    /// The text of a string scalar, or `None` for any other node.
    #[must_use]
    pub fn string(&self) -> Option<&str> {
        match &self.content {
            Content::Scalar(Value::Str(text)) => Some(text),
            _ => None,
        }
    }

    /// The flag of a boolean scalar, or `None` for any other node.
    #[must_use]
    pub const fn boolean(&self) -> Option<bool> {
        match &self.content {
            Content::Scalar(Value::Bool(flag)) => Some(*flag),
            _ => None,
        }
    }

    /// The node's canonical JSON text: no whitespace, mapping entries in
    /// source order, strings escaped as `serde_json` escapes them, integers
    /// and exact decimals as written in their canonical form, and a float
    /// in the shortest form that reads back as the same float.
    ///
    /// # Errors
    ///
    /// Returns the [`JsonTextError`] of the first scalar, in document
    /// order, that JSON cannot write.
    pub fn json_text(&self) -> Result<String, JsonTextError> {
        let mut out = String::new();
        self.write_json(&mut out)?;
        Ok(out)
    }

    fn write_json(&self, out: &mut String) -> Result<(), JsonTextError> {
        match &self.content {
            Content::Scalar(value) => write_scalar(value, out)?,
            Content::Sequence(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_json(out)?;
                }
                out.push(']');
            }
            Content::Mapping(entries) => {
                out.push('{');
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&json_string(key));
                    out.push(':');
                    value.write_json(out)?;
                }
                out.push('}');
            }
        }
        Ok(())
    }
}

/// `text` as a JSON string literal.
#[must_use]
pub fn json_string(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

fn write_scalar(value: &Value, out: &mut String) -> Result<(), JsonTextError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Int(integer) => {
            let _ = write!(out, "{integer}");
        }
        Value::BigInt(integer) => {
            let _ = write!(out, "{integer}");
        }
        Value::Decimal(decimal) => {
            let _ = write!(out, "{decimal}");
        }
        Value::Float(float) => {
            let number =
                serde_json::Number::from_f64(*float).ok_or(JsonTextError::NonFiniteNumber)?;
            let _ = write!(out, "{number}");
        }
        Value::Str(text) => out.push_str(&json_string(text)),
        Value::Bytes(_) => return Err(JsonTextError::BinaryScalar),
        _ => return Err(JsonTextError::TaggedScalar),
    }
    Ok(())
}

/// The open schema the YAML route reads any document under.
fn open_schema() -> Result<Schema, ParseInstanceError> {
    let protocol = Protocol {
        name: YAML.into(),
        schema_theory: "ThYamlSchema".into(),
        instance_theory: "ThYamlInstance".into(),
        ..Default::default()
    };
    SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .and_then(SchemaBuilder::build)
        .map_err(|error| ParseInstanceError::Parse {
            protocol: YAML.into(),
            message: error.to_string(),
        })
}

/// Read `source` through the shared YAML route into its tree.
///
/// # Errors
///
/// Returns the YAML route's [`ParseInstanceError`] for a document the route
/// refuses, or [`ParseInstanceError::Parse`] if the YAML grammar is not
/// compiled in.
pub fn read_yaml(source: &[u8]) -> Result<Node, ParseInstanceError> {
    let codec = UnifiedCodec::yaml(YAML).map_err(|error| ParseInstanceError::Parse {
        protocol: YAML.into(),
        message: error.to_string(),
    })?;
    let schema = open_schema()?;
    let (instance, complement) = codec.parse_wtype_preserving(&schema, source)?;
    let mut children: HashMap<u32, Vec<(u32, &Edge)>> = HashMap::new();
    for (parent, child, edge) in &instance.arcs {
        children.entry(*parent).or_default().push((*child, edge));
    }
    let start = usize::from(complement.byte_order_mark) * 3;
    Ok(build(
        &instance,
        &complement,
        &children,
        instance.root,
        start,
    ))
}

fn build(
    instance: &WInstance,
    complement: &CstComplement,
    children: &HashMap<u32, Vec<(u32, &Edge)>>,
    id: u32,
    start: usize,
) -> Node {
    let offset = complement
        .node_to_cst_value
        .get(&id)
        .and_then(|run| run.first())
        .or_else(|| complement.node_to_cst_struct.get(&id))
        .and_then(|vertex| cst_byte_offset(&complement.cst_schema, vertex))
        .and_then(|offset| usize::try_from(offset).ok())
        .map_or(start, |offset| offset + start);
    let node = &instance.nodes[&id];
    let kids = || children.get(&id).into_iter().flatten();
    let content = match &node.value {
        Some(FieldPresence::Present(value)) => Content::Scalar(value.clone()),
        Some(FieldPresence::Null | FieldPresence::Absent) => Content::Scalar(Value::Null),
        None if node.shape == NodeShape::List => Content::Sequence(
            kids()
                .map(|(child, _)| build(instance, complement, children, *child, start))
                .collect(),
        ),
        None => Content::Mapping(
            kids()
                .map(|(child, edge)| {
                    (
                        edge.name.as_deref().unwrap_or_default().to_owned(),
                        build(instance, complement, children, *child, start),
                    )
                })
                .collect(),
        ),
    };
    Node { content, offset }
}
