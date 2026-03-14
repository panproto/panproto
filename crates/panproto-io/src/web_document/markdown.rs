//! Markdown instance codec via `pulldown-cmark`.
//!
//! Parses Markdown text into a `WInstance` tree representing the document
//! structure (headings, paragraphs, lists, code blocks, etc.).

use std::collections::HashMap;

use pulldown_cmark::{Event, Options, Parser, Tag};

use panproto_inst::metadata::Node;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{FInstance, WInstance};
use panproto_schema::{Edge, Schema};

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// Markdown instance codec using `pulldown-cmark`.
pub struct MarkdownCodec;

impl Default for MarkdownCodec {
    fn default() -> Self {
        Self
    }
}

impl MarkdownCodec {
    /// Create a new Markdown codec.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl InstanceParser for MarkdownCodec {
    fn protocol_name(&self) -> &str {
        "markdown"
    }

    fn native_repr(&self) -> NativeRepr {
        NativeRepr::WType
    }

    fn parse_wtype(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<WInstance, ParseInstanceError> {
        let md_str = std::str::from_utf8(input).map_err(|e| ParseInstanceError::Parse {
            protocol: "markdown".into(),
            message: format!("invalid UTF-8: {e}"),
        })?;

        let options = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_FOOTNOTES;
        let parser = Parser::new_ext(md_str, options);

        let mut nodes = HashMap::new();
        let mut arcs = Vec::new();
        let mut next_id: u32 = 0;

        // Create root document node.
        let root_id = next_id;
        next_id += 1;
        let root_anchor = find_root_vertex(schema).unwrap_or_else(|| "document".into());
        nodes.insert(
            root_id,
            Node {
                id: root_id,
                anchor: root_anchor.clone(),
                value: None,
                discriminator: None,
                extra_fields: HashMap::new(),
                position: None,
                annotations: HashMap::new(),
            },
        );

        let mut element_stack: Vec<(u32, String)> = vec![(root_id, root_anchor.clone())];

        for event in parser {
            match event {
                Event::Start(tag) => {
                    let kind = tag_to_kind(&tag);
                    let node_id = next_id;
                    next_id += 1;

                    let Some(parent) = element_stack.last() else { continue };
                    let anchor = format!("{}:{kind}", parent.1);

                    nodes.insert(
                        node_id,
                        Node {
                            id: node_id,
                            anchor: anchor.clone(),
                            value: None,
                            discriminator: Some(kind.clone()),
                            extra_fields: HashMap::new(),
                            position: None,
                            annotations: HashMap::new(),
                        },
                    );

                    let edge = Edge {
                        src: parent.1.clone(),
                        tgt: anchor.clone(),
                        kind: "contains".to_string(),
                        name: Some(kind),
                    };
                    arcs.push((parent.0, node_id, edge));
                    element_stack.push((node_id, anchor));
                }
                Event::End(_) => {
                    if element_stack.len() > 1 {
                        element_stack.pop();
                    }
                }
                Event::Text(text) | Event::Code(text) => {
                    let node_id = next_id;
                    next_id += 1;

                    let Some(parent) = element_stack.last() else { continue };
                    let anchor = format!("{}:text", parent.1);

                    nodes.insert(
                        node_id,
                        Node {
                            id: node_id,
                            anchor: anchor.clone(),
                            value: Some(FieldPresence::Present(Value::Str(text.to_string()))),
                            discriminator: None,
                            extra_fields: HashMap::new(),
                            position: None,
                            annotations: HashMap::new(),
                        },
                    );

                    let edge = Edge {
                        src: parent.1.clone(),
                        tgt: anchor,
                        kind: "contains".to_string(),
                        name: Some("text".to_string()),
                    };
                    arcs.push((parent.0, node_id, edge));
                }
                Event::SoftBreak | Event::HardBreak => {}
                _ => {}
            }
        }

        Ok(WInstance::new(
            nodes,
            arcs,
            Vec::new(),
            root_id,
            root_anchor,
        ))
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: "markdown".into(),
            requested: NativeRepr::Functor,
            native: NativeRepr::WType,
        })
    }
}

impl InstanceEmitter for MarkdownCodec {
    fn protocol_name(&self) -> &str {
        "markdown"
    }

    fn emit_wtype(
        &self,
        _schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let mut output = String::new();
        emit_md_node(&mut output, instance, instance.root, 0);
        Ok(output.into_bytes())
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        _instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Err(EmitInstanceError::UnsupportedRepresentation {
            protocol: "markdown".into(),
            requested: NativeRepr::Functor,
            native: NativeRepr::WType,
        })
    }
}

/// Map a `pulldown-cmark` tag to a vertex kind string.
fn tag_to_kind(tag: &Tag<'_>) -> String {
    match tag {
        Tag::Paragraph => "paragraph".into(),
        Tag::Heading { level, .. } => format!("heading-{}", *level as u8),
        Tag::BlockQuote(_) => "blockquote".into(),
        Tag::CodeBlock(_) => "code-block".into(),
        Tag::List(Some(_)) => "ordered-list".into(),
        Tag::List(None) => "unordered-list".into(),
        Tag::Item => "list-item".into(),
        Tag::Emphasis => "emphasis".into(),
        Tag::Strong => "strong".into(),
        Tag::Strikethrough => "strikethrough".into(),
        Tag::Link { .. } => "link".into(),
        Tag::Image { .. } => "image".into(),
        Tag::Table(_) => "table".into(),
        Tag::TableHead => "table-head".into(),
        Tag::TableRow => "table-row".into(),
        Tag::TableCell => "table-cell".into(),
        Tag::FootnoteDefinition(_) => "footnote".into(),
        Tag::HtmlBlock => "html-block".into(),
        Tag::MetadataBlock(_) => "metadata-block".into(),
        Tag::DefinitionList => "definition-list".into(),
        Tag::DefinitionListTitle => "definition-title".into(),
        Tag::DefinitionListDefinition => "definition".into(),
    }
}

/// Recursively emit Markdown from a `WInstance` node.
fn emit_md_node(output: &mut String, instance: &WInstance, node_id: u32, depth: usize) {
    let Some(node) = instance.nodes.get(&node_id) else {
        return;
    };

    // Emit text content.
    if let Some(FieldPresence::Present(Value::Str(ref text))) = node.value {
        output.push_str(text);
        return;
    }

    let kind = node
        .discriminator
        .as_deref()
        .unwrap_or("");

    // Emit opening syntax based on kind.
    match kind {
        k if k.starts_with("heading-") => {
            let level: usize = k.strip_prefix("heading-").and_then(|l| l.parse().ok()).unwrap_or(1);
            for _ in 0..level {
                output.push('#');
            }
            output.push(' ');
        }
        "paragraph" => {}
        "emphasis" => output.push('*'),
        "strong" => output.push_str("**"),
        "code-block" => output.push_str("```\n"),
        "list-item" => {
            for _ in 0..depth.saturating_sub(1) {
                output.push_str("  ");
            }
            output.push_str("- ");
        }
        "blockquote" => output.push_str("> "),
        _ => {}
    }

    // Emit children.
    if let Some(children) = instance.children_map.get(&node_id) {
        for &child_id in children {
            emit_md_node(output, instance, child_id, depth + 1);
        }
    }

    // Emit closing syntax.
    match kind {
        "emphasis" => output.push('*'),
        "strong" => output.push_str("**"),
        "code-block" => output.push_str("\n```"),
        "paragraph" | "blockquote" => output.push_str("\n\n"),
        k if k.starts_with("heading-") => output.push('\n'),
        _ => {}
    }
}

fn find_root_vertex(schema: &Schema) -> Option<String> {
    schema
        .vertices
        .values()
        .find(|v| schema.incoming_edges(&v.id).is_empty())
        .map(|v| v.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

    fn md_schema() -> Schema {
        let proto = panproto_schema::Protocol {
            name: "markdown".into(),
            schema_theory: "ThMdSchema".into(),
            instance_theory: "ThMdInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["document".into(), "paragraph".into(), "text".into()],
            constraint_sorts: vec![],
            ..panproto_schema::Protocol::default()
        };
        SchemaBuilder::new(&proto)
            .vertex("doc", "document", None)
            .expect("v")
            .build()
            .expect("build")
    }

    #[test]
    fn parse_simple_markdown() {
        let codec = MarkdownCodec::new();
        let schema = md_schema();
        let input = b"# Hello\n\nWorld";
        let instance = codec.parse_wtype(&schema, input).expect("parse");
        assert!(instance.node_count() >= 3, "doc + heading + text nodes");
    }

    #[test]
    fn roundtrip_markdown() {
        let codec = MarkdownCodec::new();
        let schema = md_schema();
        let input = b"# Hello\n\nWorld\n";
        let instance = codec.parse_wtype(&schema, input).expect("parse");
        let emitted = codec.emit_wtype(&schema, &instance).expect("emit");
        let instance2 = codec.parse_wtype(&schema, &emitted).expect("re-parse");
        // Markdown round-trips may not be byte-identical, but structure should match.
        assert_eq!(instance.node_count(), instance2.node_count());
    }
}
